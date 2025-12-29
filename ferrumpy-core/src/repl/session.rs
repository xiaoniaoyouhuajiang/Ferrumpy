//! REPL Session
//!
//! Manages an evcxr evaluation context with captured debug state.

use anyhow::{Context, Result};
use crossbeam_channel::Receiver;
use evcxr::{CommandContext, Error as EvcxrError, EvalContext};
use std::path::Path;
use std::process::Command;

/// A REPL session that wraps evcxr's CommandContext
pub struct ReplSession {
    context: CommandContext,
    stdout: Receiver<String>,
    stderr: Receiver<String>,
    project_path: Option<String>,
    initialized: bool,
}

impl ReplSession {
    /// Create a new REPL session using ferrumpy-repl-worker as subprocess
    pub fn new() -> Result<Self> {
        // Find the ferrumpy-repl-worker binary
        let worker_path = Self::find_worker_binary()?;

        // Use with_subprocess_command to specify our worker binary
        // The worker has runtime_hook() called at startup
        let cmd = Command::new(&worker_path);

        let (eval_context, outputs) = EvalContext::with_subprocess_command(cmd)
            .map_err(|e| anyhow::anyhow!("Failed to create evcxr context with worker: {:?}", e))?;

        let context = CommandContext::with_eval_context(eval_context);

        // Using default LLVM backend
        // Note: Cranelift was tested but showed higher wall-clock time despite lower CPU usage
        // (LLVM: 22.9s total vs Cranelift: 27.6s total)
        eprintln!("[FerrumPy] Using LLVM backend");

        let mut session = Self {
            context,
            stdout: outputs.stdout,
            stderr: outputs.stderr,
            project_path: None,
            initialized: false,
        };

        // Enable dependency caching (512MB) for faster subsequent starts
        // Cache persists in ~/Library/Caches/evcxr/ (macOS) or equivalent
        if let Err(e) = session.context.execute(":cache 512") {
            eprintln!("[FerrumPy] Warning: Failed to enable cache: {:?}", e);
        } else {
            eprintln!("[FerrumPy] Cache enabled (512MB)");
        }

        Ok(session)
    }

    /// Find the ferrumpy-repl-worker binary
    fn find_worker_binary() -> Result<String> {
        // Try several locations in order:
        // 1. Same directory as this .so module (pip install location)
        // 2. PATH
        // 3. Cargo target directory (for development)

        let worker_name = if cfg!(windows) {
            "ferrumpy-repl-worker.exe"
        } else {
            "ferrumpy-repl-worker"
        };

        // Check same directory as this module (where pip installs it)
        // The binary and .so will be in the same directory: site-packages/ferrumpy/
        if let Some(module_dir) = Self::get_module_directory() {
            let worker = module_dir.join(worker_name);
            if worker.exists() {
                return Ok(worker.to_string_lossy().to_string());
            }
        }

        // Check PATH
        if let Some(path) = Self::find_in_path(worker_name) {
            return Ok(path);
        }

        // Development fallback: check cargo target directory
        // Try absolute path first
        let project_root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());

        for profile in ["debug", "release"] {
            // Check workspace target directory
            let worker = format!("{}/target/{}/{}", project_root, profile, worker_name);
            if std::path::Path::new(&worker).exists() {
                return Ok(worker);
            }

            // Check current working directory target
            let worker = format!("target/{}/{}", profile, worker_name);
            if std::path::Path::new(&worker).exists() {
                return Ok(std::fs::canonicalize(&worker)?
                    .to_string_lossy()
                    .to_string());
            }
        }

        Err(anyhow::anyhow!(
            "Could not find ferrumpy-repl-worker binary. \
             Expected in: site-packages/ferrumpy/ or target/debug/"
        ))
    }

    /// Get the directory containing this module (.so file)
    fn get_module_directory() -> Option<std::path::PathBuf> {
        // Try to get the path of the current shared library
        // This works for dynamically loaded modules
        #[cfg(unix)]
        {
            use std::ffi::CStr;

            extern "C" {
                fn dladdr(addr: *const std::ffi::c_void, info: *mut DlInfo) -> std::ffi::c_int;
            }

            #[repr(C)]
            struct DlInfo {
                dli_fname: *const std::ffi::c_char,
                dli_fbase: *const std::ffi::c_void,
                dli_sname: *const std::ffi::c_char,
                dli_saddr: *const std::ffi::c_void,
            }

            let mut info: DlInfo = unsafe { std::mem::zeroed() };
            let func_ptr = Self::get_module_directory as *const std::ffi::c_void;

            if unsafe { dladdr(func_ptr, &mut info) } != 0 && !info.dli_fname.is_null() {
                let path_str = unsafe { CStr::from_ptr(info.dli_fname) };
                if let Ok(path) = path_str.to_str() {
                    return std::path::Path::new(path).parent().map(|p| p.to_path_buf());
                }
            }
        }

        None
    }

    /// Search for a binary in PATH
    fn find_in_path(name: &str) -> Option<String> {
        let path_var = std::env::var("PATH").ok()?;
        let separator = if cfg!(windows) { ';' } else { ':' };

        for dir in path_var.split(separator) {
            let candidate = std::path::Path::new(dir).join(name);
            if candidate.exists() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
        None
    }

    /// Create a new REPL session with a project dependency
    pub fn with_project(project_path: &Path) -> Result<Self> {
        let mut session = Self::new()?;
        session.project_path = Some(project_path.to_string_lossy().to_string());
        Ok(session)
    }

    /// Add a crate dependency
    pub fn add_dep(&mut self, name: &str, spec: &str) -> Result<String> {
        let dep_cmd = format!(":dep {} = {}", name, spec);
        self.eval(&dep_cmd)
    }

    /// Add a path dependency (for user's lib crate)
    pub fn add_path_dep(&mut self, name: &str, path: &Path) -> Result<String> {
        let dep_cmd = format!(":dep {} = {{ path = \"{}\" }}", name, path.display());
        self.eval(&dep_cmd)
    }

    /// Add a path dependency silently (no compilation until next eval)
    pub fn add_path_dep_silent(&mut self, name: &str, path: &Path) -> Result<()> {
        let config = format!(r#"{{ path = "{}" }}"#, path.display());
        self.context
            .add_dep_silent(name, &config)
            .map_err(|e| anyhow::anyhow!("Failed to add path dep: {:?}", e))
    }

    /// Load variables from serialized JSON snapshot using optimized single-compilation mode
    /// with TYPE-AWARE code generation for real Rust types
    pub fn load_snapshot(&mut self, json_data: &str, _type_hints: &str) -> Result<String> {
        // OPTIMIZED: Use add_dep_silent to register deps without triggering compilation.
        // All deps are batched, then compiled once with the final code.

        // Step 1: Register dependencies silently (no compilation yet)
        self.context
            .add_dep_silent("serde", r#"{ version = "1", features = ["derive"] }"#)
            .map_err(|e| anyhow::anyhow!("Failed to add serde dep: {:?}", e))?;
        self.context
            .add_dep_silent("serde_json", r#""1""#)
            .map_err(|e| anyhow::anyhow!("Failed to add serde_json dep: {:?}", e))?;

        // Step 2: Parse the JSON snapshot
        let snapshot: serde_json::Value =
            serde_json::from_str(json_data).context("Failed to parse snapshot JSON")?;

        // Get types map for type-aware code generation
        let types_map = snapshot.get("types").and_then(|v| v.as_object());

        // Step 3: Build all code in one batch
        let mut all_code = String::new();

        // Add companion lib use statement if present (from Python layer)
        if let Some(lib_use) = snapshot.get("lib_use_stmt").and_then(|v| v.as_str()) {
            all_code.push_str(lib_use);
            all_code.push('\n');
        }

        // Add standard library imports for common types
        all_code.push_str("use std::sync::Arc;\n");
        all_code.push_str("use std::rc::Rc;\n");
        all_code.push_str("use std::collections::HashMap;\n");

        // Add serde imports (still needed for user types)
        all_code.push_str("use serde::{Serialize, Deserialize};\n");

        // Add variable declarations with TYPE-AWARE code generation
        let mut loaded_vars: Vec<String> = Vec::new();
        if let Some(variables) = snapshot.get("variables") {
            if let Some(vars) = variables.as_object() {
                for (name, value) in vars {
                    // Get the type hint for this variable
                    let type_hint = types_map
                        .and_then(|m| m.get(name))
                        .and_then(|v| v.as_str())
                        .unwrap_or("serde_json::Value");

                    // Determine the actual type to use
                    // For unsupported types, fallback to serde_json::Value (don't skip!)
                    let mut actual_type = if self.is_supported_type(type_hint) {
                        // Transform type hint: clean up and normalize
                        self.normalize_rust_type(type_hint)
                    } else {
                        // Fallback: use generic JSON value
                        "serde_json::Value".to_string()
                    };

                    // Check if the serialized value is valid for the type
                    // E.g., empty strings or error markers like __result__ can't be deserialized
                    if !self.is_valid_for_deserialization(value, &actual_type) {
                        actual_type = "serde_json::Value".to_string();
                    }

                    let code = self.generate_typed_var_code(name, value, &actual_type)?;
                    all_code.push_str(&code);
                    all_code.push('\n');
                    loaded_vars.push(format!("{}: {}", name, actual_type));
                }
            }
        }

        // Step 4: Single compilation with all code
        if !all_code.is_empty() {
            self.eval(&all_code)?;
        }

        self.initialized = true;

        // Return list of loaded variables
        if loaded_vars.is_empty() {
            Ok("Snapshot loaded (no variables loaded - all types filtered)".to_string())
        } else {
            Ok(format!(
                "Snapshot loaded with {} variables: {}",
                loaded_vars.len(),
                loaded_vars.join(", ")
            ))
        }
    }

    /// Generate type-aware variable declaration code
    fn generate_typed_var_code(
        &self,
        name: &str,
        value: &serde_json::Value,
        type_hint: &str,
    ) -> Result<String> {
        // Handle primitive types with literal generation
        match type_hint {
            // Integer types - generate literal
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
            | "u128" | "usize" => {
                if let Some(n) = value.as_i64() {
                    return Ok(format!("let {}: {} = {};", name, type_hint, n));
                } else if let Some(n) = value.as_u64() {
                    return Ok(format!("let {}: {} = {};", name, type_hint, n));
                }
            }

            // Float types
            "f32" | "f64" => {
                if let Some(f) = value.as_f64() {
                    return Ok(format!("let {}: {} = {:.15};", name, type_hint, f));
                }
            }

            // Boolean
            "bool" => {
                if let Some(b) = value.as_bool() {
                    return Ok(format!("let {}: bool = {};", name, b));
                }
            }

            // String - generate with to_string()
            "String" => {
                if let Some(s) = value.as_str() {
                    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                    return Ok(format!(
                        "let {}: String = \"{}\".to_string();",
                        name, escaped
                    ));
                }
            }

            // Vec types - generate vec![] macro
            t if t.starts_with("Vec<") => {
                if let Some(arr) = value.as_array() {
                    // Extract inner type
                    let inner_type = &t[4..t.len() - 1];
                    let elements = self.generate_vec_elements(arr, inner_type)?;
                    return Ok(format!("let {}: {} = vec![{}];", name, t, elements));
                }
            }

            // Option types
            t if t.starts_with("Option<") => {
                let inner_type = &t[7..t.len() - 1];
                if value.is_null() {
                    return Ok(format!("let {}: {} = None;", name, t));
                } else {
                    let inner_code = self.generate_value_expr(value, inner_type)?;
                    return Ok(format!("let {}: {} = Some({});", name, t, inner_code));
                }
            }

            _ => {}
        }

        // Fallback: use serde_json for complex/user types
        let json_str = serde_json::to_string(value)?;
        // Use type annotation for proper deserialization
        if type_hint != "serde_json::Value" && type_hint != "?" {
            Ok(format!(
                "let {}: {} = serde_json::from_str(r#\"{}\"#).unwrap();",
                name, type_hint, json_str
            ))
        } else {
            Ok(format!(
                "let {} = serde_json::from_str::<serde_json::Value>(r#\"{}\"#).unwrap();",
                name, json_str
            ))
        }
    }

    /// Generate vec elements as comma-separated expressions
    fn generate_vec_elements(&self, arr: &[serde_json::Value], inner_type: &str) -> Result<String> {
        let elements: Result<Vec<String>> = arr
            .iter()
            .map(|v| self.generate_value_expr(v, inner_type))
            .collect();
        Ok(elements?.join(", "))
    }

    /// Generate a value expression for a given type
    fn generate_value_expr(&self, value: &serde_json::Value, type_hint: &str) -> Result<String> {
        match type_hint {
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => {
                Ok(value.as_i64().map(|n| n.to_string()).unwrap_or("0".into()))
            }
            "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => {
                Ok(value.as_u64().map(|n| n.to_string()).unwrap_or("0".into()))
            }
            "f32" | "f64" => Ok(value
                .as_f64()
                .map(|f| format!("{:.15}", f))
                .unwrap_or("0.0".into())),
            "bool" => Ok(value
                .as_bool()
                .map(|b| b.to_string())
                .unwrap_or("false".into())),
            "String" => {
                let s = value.as_str().unwrap_or("");
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                Ok(format!("\"{}\".to_string()", escaped))
            }
            _ => {
                // For complex types, use serde
                let json_str = serde_json::to_string(value)?;
                Ok(format!(
                    "serde_json::from_str::<{}>(r#\"{}\"#).unwrap()",
                    type_hint, json_str
                ))
            }
        }
    }

    /// Fix user type paths: remove original crate name prefix
    /// e.g., "Arc<rust_sample::User>" -> "Arc<User>"
    fn fix_user_type_path(&self, type_hint: &str) -> String {
        // Remove the original crate name prefix (keep the type name)
        // Match pattern: some_crate::TypeName -> TypeName
        let mut result = type_hint.to_string();

        // Simple approach: strip everything before the last ::
        // This handles "rust_sample::User" -> "User"
        // And "Arc<rust_sample::User>" -> "Arc<User>" via regex-like replacement
        while let Some(start) = result.find("::") {
            // Find the word before ::
            let before = &result[..start];
            if let Some(word_start) = before.rfind(|c: char| !c.is_alphanumeric() && c != '_') {
                let prefix = &result[word_start + 1..=start + 1];
                result = result.replacen(prefix, "", 1);
            } else {
                // Word at start
                let prefix = &result[..=start + 1];
                result = result.replacen(prefix, "", 1);
            }
        }

        result
    }

    /// Check if a type is supported for snapshot restoration
    /// With improved type normalization from Python, we can now support more types
    fn is_supported_type(&self, type_hint: &str) -> bool {
        // Skip pointer types (raw pointers)
        if type_hint.contains(" *") || type_hint.contains("*const") || type_hint.contains("*mut") {
            return false;
        }

        // Skip references (can't deserialize references)
        if type_hint.starts_with("&") {
            return false;
        }

        // Skip smart pointers (Arc/Rc/Box don't implement Deserialize by default)
        // These require special serde features to deserialize
        if type_hint.contains("Arc<") || type_hint.contains("Rc<") || type_hint.contains("Box<") {
            return false;
        }

        // Skip RefCell/Cell (complex internal state, can't deserialize)
        if type_hint.contains("RefCell<") || type_hint.contains("Cell<") {
            return false;
        }

        // Skip allocator types (should be normalized away by Python, but double-check)
        if type_hint.contains("Global") || type_hint.contains("alloc::") {
            return false;
        }

        // Skip C-style arrays (int[5] format - should be normalized, but check)
        if type_hint.contains("[") && type_hint.contains("]") && !type_hint.starts_with("Vec") {
            return false;
        }

        // Skip tuples containing references (e.g., (&str, i32))
        if type_hint.starts_with("(") && type_hint.contains("&") {
            return false;
        }

        // Skip unknown types
        if type_hint == "?" || type_hint.is_empty() {
            return false;
        }

        // NOW ALLOW:
        // - Result<T, E> (serde can deserialize)
        // - HashMap<K, V> (serde can deserialize)
        // - Nested generics Vec<Vec<T>> (now properly normalized)
        // - User-defined types (Config, User etc. from companion lib)
        // - Tuples without references (T1, T2, T3)

        true
    }

    /// Check if a serialized value is valid for deserialization to the target type
    /// Returns false for values that would cause serde::from_str to panic
    fn is_valid_for_deserialization(&self, value: &serde_json::Value, type_hint: &str) -> bool {
        // Empty strings cannot be deserialized to most types
        if let Some(s) = value.as_str() {
            if s.is_empty() {
                return false;
            }
        }

        // Check for error marker objects from Python serializer
        if let Some(obj) = value.as_object() {
            for key in obj.keys() {
                // These markers indicate serialization failed in Python
                if key.starts_with("__") && key.ends_with("__") {
                    return false;
                }
            }

            // For Result<T, E>, need {"Ok": ...} or {"Err": ...} format
            if type_hint.starts_with("Result<") {
                if !obj.contains_key("Ok") && !obj.contains_key("Err") {
                    return false;
                }
            }

            // For HashMap<K, V>, the object should be a simple key-value map
            // (not containing error markers, which we already checked)
        }

        // serde_json::Value can always be deserialized
        if type_hint == "serde_json::Value" {
            return true;
        }

        true
    }

    /// Normalize Rust type: remove allocator, convert C types, strip crate prefixes
    fn normalize_rust_type(&self, type_hint: &str) -> String {
        let mut result = type_hint.to_string();

        // Remove Global allocator from generics
        // Vec<i32, alloc::alloc::Global> -> Vec<i32>
        result = result.replace(", alloc::alloc::Global", "");
        result = result.replace(",alloc::alloc::Global", "");
        result = result.replace(", Global", "");
        result = result.replace(",Global", "");

        // Convert C types to Rust types
        result = result.replace("int", "i32");
        result = result.replace("unsigned long", "u64");
        result = result.replace("long", "i64");
        result = result.replace("unsigned short", "u16");
        result = result.replace("short", "i16");
        result = result.replace("unsigned char", "u8");
        result = result.replace("double", "f64");
        result = result.replace("float", "f32");

        // Strip crate prefixes (rust_sample::User -> User)
        result = self.fix_user_type_path(&result);

        result
    }

    /// Evaluate a Rust expression
    pub fn eval(&mut self, code: &str) -> Result<String> {
        // Use CommandContext::execute instead of EvalContext::eval
        let outputs = self.context.execute(code).map_err(|e| match e {
            EvcxrError::CompilationErrors(errors) => {
                // Prioritize errors over warnings to avoid important errors being hidden
                let mut formatted = String::new();

                // First, collect and show errors
                for err in errors.iter().filter(|e| e.level() == "error") {
                    formatted.push_str(&err.rendered());
                    formatted.push('\n');
                }

                // Then, show warnings (limit to avoid overwhelming output)
                let warnings: Vec<_> = errors.iter().filter(|e| e.level() == "warning").collect();
                if !warnings.is_empty() {
                    if warnings.len() <= 3 {
                        for err in &warnings {
                            formatted.push_str(&err.rendered());
                            formatted.push('\n');
                        }
                    } else {
                        // Show first 2 warnings and a summary
                        for err in warnings.iter().take(2) {
                            formatted.push_str(&err.rendered());
                            formatted.push('\n');
                        }
                        formatted
                            .push_str(&format!("... and {} more warnings\n", warnings.len() - 2));
                    }
                }

                anyhow::anyhow!("{}", formatted.trim())
            }
            EvcxrError::SubprocessTerminated(msg) => {
                anyhow::anyhow!("Subprocess terminated: {}", msg)
            }
            other => anyhow::anyhow!("Eval error: {:?}", other),
        })?;

        // Collect any output from the internal stdout/stderr
        let mut result = String::new();

        // Get content_by_mime_type for the result
        if let Some(text) = outputs.content_by_mime_type.get("text/plain") {
            result.push_str(text);
        }

        // Also check for stdout from the channels
        while let Ok(line) = self.stdout.try_recv() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&line);
        }

        Ok(result)
    }

    /// Get any stderr output
    pub fn get_stderr(&self) -> Vec<String> {
        let mut errors = Vec::new();
        while let Ok(line) = self.stderr.try_recv() {
            errors.push(line);
        }
        errors
    }

    /// Check if the session is initialized with a snapshot
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get available variables (if tracked)
    pub fn variables(&self) -> Vec<String> {
        // Note: evcxr doesn't expose defined variables directly
        // We would need to track them ourselves
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session() {
        // This test requires a full Rust toolchain
        // Skip in CI if evcxr fails to initialize
        match ReplSession::new() {
            Ok(session) => {
                assert!(!session.is_initialized());
            }
            Err(e) => {
                eprintln!("Skipping test (evcxr unavailable): {}", e);
            }
        }
    }
}
