//! REPL Session
//!
//! Manages an evcxr evaluation context with captured debug state.

use anyhow::Result;
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
    // Snapshot data for preservation across interrupts
    snapshot_json: Option<String>,
    snapshot_type_hints: Option<String>,
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
            snapshot_json: None,
            snapshot_type_hints: None,
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
        // Try locations in order of priority:
        // 1. Environment variable (for manual override/testing)
        // 2. Same directory as this .so module (pip install location)
        // 3. PATH (system-wide installation)
        // 4. Current directory's target/ (development only, no recursion)

        // 1. Check environment variable
        if let Ok(path) = std::env::var("FERRUMPY_REPL_WORKER") {
            if std::path::Path::new(&path).exists() {
                return Ok(std::fs::canonicalize(path)?.to_string_lossy().to_string());
            }
        }

        let worker_name = if cfg!(windows) {
            "ferrumpy-repl-worker.exe"
        } else {
            "ferrumpy-repl-worker"
        };

        // 2. Check same directory as this module (distribution)
        if let Some(module_dir) = Self::get_module_directory() {
            let worker = module_dir.join(worker_name);
            if worker.exists() {
                return Ok(std::fs::canonicalize(worker)?.to_string_lossy().to_string());
            }
        }

        // 3. Check PATH
        if let Some(path) = Self::find_in_path(worker_name) {
            return Ok(path);
        }

        // 4. Development fallback: check current directory's target/ only
        if let Ok(cwd) = std::env::current_dir() {
            // Prefer release, then debug
            for profile in ["release", "debug"] {
                let worker = cwd.join("target").join(profile).join(worker_name);
                if worker.exists() {
                    return Ok(std::fs::canonicalize(worker)?.to_string_lossy().to_string());
                }
            }
        }

        Err(anyhow::anyhow!(
            "Could not find ferrumpy-repl-worker binary. \
             Expected locations:\n\
             - FERRUMPY_REPL_WORKER environment variable\n\
             - Same directory as ferrumpy module (site-packages/ferrumpy/)\n\
             - System PATH\n\
             - ./target/{{release,debug}}/ (development)\n\
             \n\
             Hint: Set FERRUMPY_REPL_WORKER=/path/to/ferrumpy-repl-worker for manual override."
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

    // ============================================================================
    // Snapshot Item-Level Export Helpers
    // ============================================================================

    /// Extract variables from snapshot JSON for item-level module generation
    fn extract_variables(
        &self,
        snapshot: &serde_json::Value,
    ) -> Result<Vec<(String, serde_json::Value, String)>> {
        let mut vars = Vec::new();

        let variables = snapshot
            .get("variables")
            .and_then(|v| v.as_object())
            .ok_or_else(|| anyhow::anyhow!("No variables in snapshot"))?;

        let types_map = snapshot.get("types").and_then(|v| v.as_object());

        for (name, value) in variables {
            let type_hint: &str = types_map
                .and_then(|m| m.get(name))
                .and_then(|v| v.as_str())
                .unwrap_or("serde_json::Value");

            let actual_type = if self.is_supported_type(type_hint) {
                self.normalize_rust_type(type_hint)
            } else {
                "serde_json::Value".to_string()
            };

            if !self.is_valid_for_deserialization(value, &actual_type) {
                vars.push((name.clone(), value.clone(), "serde_json::Value".to_string()));
            } else {
                vars.push((name.clone(), value.clone(), actual_type));
            }
        }

        Ok(vars)
    }

    /// Generate module containing snapshot variables as static items
    fn generate_snapshot_module(
        &self,
        vars: &[(String, serde_json::Value, String)],
    ) -> Result<String> {
        // Use different name to avoid conflict with companion lib
        let mut module_code = String::from("mod ferrumpy_vars {\n");
        module_code.push_str("    use std::sync::OnceLock;\n");
        module_code.push_str("    use std::collections::HashMap;\n");
        module_code.push_str("    use std::sync::Arc;\n");
        module_code.push_str("    use std::rc::Rc;\n");
        // Import types from parent scope (user-defined types, companion lib types)
        module_code.push_str("    use super::*;\n\n");

        for (name, value, ty) in vars {
            let item_code = self.generate_static_item(name, value, ty)?;
            module_code.push_str(&item_code);
            module_code.push_str("\n");
        }

        module_code.push_str("}\n");
        Ok(module_code)
    }

    /// Generate a single static item with accessor function
    fn generate_static_item(
        &self,
        name: &str,
        value: &serde_json::Value,
        type_hint: &str,
    ) -> Result<String> {
        let cell_name = format!("{}_CELL", name.to_uppercase());
        let init_expr = self.generate_value_init_expr(value, type_hint)?;

        let is_sync = self.is_likely_sync_type(type_hint);

        if is_sync {
            Ok(format!(
                r#"    static {}: OnceLock<{}> = OnceLock::new();
    
    pub fn {}() -> &'static {} {{
        {}.get_or_init(|| {{
            {}
        }})
    }}
"#,
                cell_name, type_hint, name, type_hint, cell_name, init_expr
            ))
        } else {
            Ok(format!(
                r#"    thread_local! {{
        static {}: std::cell::RefCell<Option<{}>> = 
            std::cell::RefCell::new(None);
    }}
    
    pub fn {}() -> {} {{
        {}.with(|cell| {{
            let mut opt = cell.borrow_mut();
            if opt.is_none() {{
                *opt = Some({});
            }}
            opt.as_ref().unwrap().clone()
        }})
    }}
"#,
                cell_name, type_hint, name, type_hint, cell_name, init_expr
            ))
        }
    }

    /// Heuristic to detect if a type is likely Sync
    fn is_likely_sync_type(&self, type_hint: &str) -> bool {
        let non_sync_markers = ["Rc<", "RefCell<", "Cell<", "*const", "*mut"];

        for marker in &non_sync_markers {
            if type_hint.contains(marker) {
                return false;
            }
        }

        true
    }

    /// Load variables from serialized JSON snapshot using optimized single-compilation mode
    /// with TYPE-AWARE code generation for real Rust types
    pub fn load_snapshot(&mut self, json_data: &str, type_hints: &str) -> Result<String> {
        // Save snapshot data for potential restoration after interrupt
        self.snapshot_json = Some(json_data.to_string());
        self.snapshot_type_hints = Some(type_hints.to_string());
        eprintln!(
            "[DEBUG] Saved snapshot: {} bytes JSON, {} bytes hints",
            json_data.len(),
            type_hints.len()
        );

        // Step 1: Register dependencies silently (no compilation yet)
        self.context
            .add_dep_silent("serde", r#"{ version = "1", features = ["derive"] }"#)
            .map_err(|e| anyhow::anyhow!("Failed to add serde dep: {:?}", e))?;
        self.context
            .add_dep_silent("serde_json", r#""1""#)
            .map_err(|e| anyhow::anyhow!("Failed to add serde_json dep: {:?}", e))?;

        // Step 2: Parse snapshot JSON
        let snapshot: serde_json::Value = serde_json::from_str(json_data)?;

        // ========== ITEM-LEVEL EXPORT PATH ==========
        if std::env::var("FERRUMPY_DEBUG").is_ok() {
            eprintln!("[DEBUG] Using item-level snapshot export");
        }

        let vars = self.extract_variables(&snapshot)?;
        if vars.is_empty() {
            return Ok("Snapshot loaded (no variables)".to_string());
        }

        let mut all_code = String::new();

        // Re-add companion library if it exists in the snapshot
        if let Some(lib_path) = snapshot.get("lib_path").and_then(|v| v.as_str()) {
            let lib_name = snapshot
                .get("lib_name")
                .and_then(|v| v.as_str())
                .unwrap_or("ferrumpy_snapshot");
            self.add_path_dep_silent(lib_name, Path::new(lib_path))?;
        }

        if let Some(lib_use) = snapshot.get("lib_use_stmt").and_then(|v| v.as_str()) {
            all_code.push_str(lib_use);
            all_code.push('\n');
        }
        all_code.push_str("use serde::{Serialize, Deserialize};\n");
        let module_code = self.generate_snapshot_module(&vars)?;
        all_code.push_str(&module_code);
        all_code.push('\n');
        all_code.push_str("use ferrumpy_vars::*;\n");

        self.eval(&all_code)?;
        self.initialized = true;

        let sample_names: Vec<&str> = vars.iter().take(5).map(|(n, _, _)| n.as_str()).collect();
        Ok(format!(
            "Snapshot loaded with {} items. Access: {}{}",
            vars.len(),
            sample_names
                .iter()
                .map(|n| format!("{}()", n))
                .collect::<Vec<_>>()
                .join(", "),
            if vars.len() > 5 { ", ..." } else { "" }
        ))
    }

    /// Generate initialization expression for a value (for let bindings)
    fn generate_value_init_expr(
        &self,
        value: &serde_json::Value,
        type_hint: &str,
    ) -> Result<String> {
        // Check for __ferrumpy_kind__ metadata (special type handling)
        // BUT: Skip if type_hint is serde_json::Value (fallback case for unsupported types)
        if type_hint != "serde_json::Value" {
            if let Some(kind) = value.get("__ferrumpy_kind__").and_then(|v| v.as_str()) {
                return match kind {
                    "option" => self.generate_option_code(value, type_hint),
                    "result" => self.generate_result_code(value, type_hint),
                    "tuple" => self.generate_tuple_code(value, type_hint),
                    "array" => self.generate_array_code(value, type_hint),
                    "arc" => self.generate_arc_code(value, type_hint),
                    "rc" => self.generate_rc_code(value, type_hint),
                    "box" => self.generate_box_code(value, type_hint),
                    "enum" => self.generate_enum_code(value, type_hint),
                    _ => {
                        // Unknown kind, fall through to default handling
                        let json_str = serde_json::to_string(value)?;
                        Ok(format!(
                            "serde_json::from_str::<serde_json::Value>(r#\"{}\"#).unwrap()",
                            json_str
                        ))
                    }
                };
            }
        }

        // Handle primitive types with literal generation
        match type_hint {
            // Integer types - generate literal
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
            | "u128" | "usize" => {
                if let Some(n) = value.as_i64() {
                    return Ok(format!("{}{}", n, self.type_suffix(type_hint)));
                } else if let Some(n) = value.as_u64() {
                    return Ok(format!("{}{}", n, self.type_suffix(type_hint)));
                }
            }

            // Float types
            "f32" | "f64" => {
                if let Some(f) = value.as_f64() {
                    return Ok(format!("{:.15}{}", f, self.type_suffix(type_hint)));
                }
            }

            // Boolean
            "bool" => {
                if let Some(b) = value.as_bool() {
                    return Ok(b.to_string());
                }
            }

            // String - generate with to_string()
            "String" => {
                if let Some(s) = value.as_str() {
                    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                    return Ok(format!("\"{}\".to_string()", escaped));
                }
            }

            // Vec types - generate vec![] macro
            t if t.starts_with("Vec<") => {
                if let Some(arr) = value.as_array() {
                    // Extract inner type
                    let inner_type = &t[4..t.len() - 1];
                    let elements = self.generate_vec_elements(arr, inner_type)?;
                    return Ok(format!("vec![{}]", elements));
                }
            }

            // Option types (legacy path - now handled by __ferrumpy_kind__)
            t if t.starts_with("Option<") => {
                let inner_type = &t[7..t.len() - 1];
                if value.is_null() {
                    return Ok("None".to_string());
                } else {
                    let inner_code = self.generate_value_expr(value, inner_type)?;
                    return Ok(format!("Some({})", inner_code));
                }
            }

            _ => {}
        }

        // Fallback: use serde_json for complex/user types
        let json_str = serde_json::to_string(value)?;
        // Use type annotation for proper deserialization
        if type_hint != "serde_json::Value" && type_hint != "?" {
            Ok(format!(
                "serde_json::from_str::<{}>(r#\"{}\"#).unwrap()",
                type_hint, json_str
            ))
        } else {
            Ok(format!(
                "serde_json::from_str::<serde_json::Value>(r#\"{}\"#).unwrap()",
                json_str
            ))
        }
    }

    /// Generate code for Option<T> from __ferrumpy_kind__ metadata
    fn generate_option_code(&self, value: &serde_json::Value, type_hint: &str) -> Result<String> {
        let variant = value
            .get("__variant__")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        match variant {
            "None" => Ok("None".to_string()),
            "Some" => {
                // Extract inner type from Option<T>
                let inner_type = if type_hint.starts_with("Option<") && type_hint.ends_with(">") {
                    &type_hint[7..type_hint.len() - 1]
                } else {
                    "serde_json::Value"
                };

                if let Some(inner) = value.get("__inner__") {
                    let inner_code = self.generate_value_init_expr(inner, inner_type)?;
                    Ok(format!("Some({})", inner_code))
                } else {
                    Ok("None".to_string())
                }
            }
            _ => Ok("None".to_string()), // Unknown variant, default to None
        }
    }

    /// Generate code for Result<T, E> from __ferrumpy_kind__ metadata
    fn generate_result_code(&self, value: &serde_json::Value, type_hint: &str) -> Result<String> {
        let variant = value
            .get("__variant__")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        // Parse Result<T, E> to extract T and E types
        let (ok_type, err_type) = self.parse_result_types(type_hint);

        match variant {
            "Ok" => {
                if let Some(inner) = value.get("__inner__") {
                    let inner_code = self.generate_value_init_expr(inner, &ok_type)?;
                    Ok(format!("Ok({})", inner_code))
                } else {
                    Ok("Ok(())".to_string())
                }
            }
            "Err" => {
                if let Some(inner) = value.get("__inner__") {
                    let inner_code = self.generate_value_init_expr(inner, &err_type)?;
                    Ok(format!("Err({})", inner_code))
                } else {
                    Ok("Err(\"unknown\".to_string())".to_string())
                }
            }
            _ => Ok("Err(\"unknown variant\".to_string())".to_string()),
        }
    }

    /// Parse Result<T, E> to extract T and E
    fn parse_result_types(&self, type_hint: &str) -> (String, String) {
        if !type_hint.starts_with("Result<") || !type_hint.ends_with(">") {
            return ("serde_json::Value".to_string(), "String".to_string());
        }

        let inner = &type_hint[7..type_hint.len() - 1];
        // Simple split on ", " - may not work for nested generics
        if let Some(comma_pos) = inner.find(", ") {
            let ok_type = inner[..comma_pos].to_string();
            let err_type = inner[comma_pos + 2..].to_string();
            (ok_type, err_type)
        } else {
            (inner.to_string(), "String".to_string())
        }
    }

    /// Generate code for tuple from __ferrumpy_kind__ metadata
    fn generate_tuple_code(&self, value: &serde_json::Value, type_hint: &str) -> Result<String> {
        let elements = value.get("__elements__").and_then(|v| v.as_array());

        if let Some(elems) = elements {
            // Parse tuple types from type_hint like "(i32, String, f64)"
            let elem_types = self.parse_tuple_types(type_hint);

            let mut parts = Vec::new();
            for (i, elem) in elems.iter().enumerate() {
                let elem_type = elem_types
                    .get(i)
                    .map(|s| s.as_str())
                    .unwrap_or("serde_json::Value");
                let part = self.generate_value_init_expr(elem, elem_type)?;
                parts.push(part);
            }

            Ok(format!("({})", parts.join(", ")))
        } else {
            Ok("()".to_string())
        }
    }

    /// Parse tuple type "(T1, T2, T3)" into vec of types
    fn parse_tuple_types(&self, type_hint: &str) -> Vec<String> {
        if !type_hint.starts_with("(") || !type_hint.ends_with(")") {
            return vec![];
        }

        let inner = &type_hint[1..type_hint.len() - 1];
        // Simple split - may not work for nested generics with commas
        inner.split(", ").map(|s| s.to_string()).collect()
    }

    /// Generate code for fixed array from __ferrumpy_kind__ metadata
    fn generate_array_code(&self, value: &serde_json::Value, type_hint: &str) -> Result<String> {
        let elements = value.get("__elements__").and_then(|v| v.as_array());

        if let Some(elems) = elements {
            // Parse array type like "[i32; 5]" to get element type
            let elem_type = self.parse_array_elem_type(type_hint);

            let mut parts = Vec::new();
            for elem in elems {
                let part = self.generate_value_init_expr(elem, &elem_type)?;
                parts.push(part);
            }

            Ok(format!("[{}]", parts.join(", ")))
        } else {
            Ok("[]".to_string())
        }
    }

    /// Parse array type "[T; N]" to get element type T
    fn parse_array_elem_type(&self, type_hint: &str) -> String {
        // Handle "[i32; 5]" format
        if type_hint.starts_with("[") && type_hint.contains(";") {
            if let Some(semi_pos) = type_hint.find(';') {
                return type_hint[1..semi_pos].trim().to_string();
            }
        }
        // Handle "int[5]" C-style format
        if let Some(bracket_pos) = type_hint.find('[') {
            return type_hint[..bracket_pos].to_string();
        }
        "serde_json::Value".to_string()
    }

    /// Generate code for Arc<T> from __ferrumpy_kind__ metadata
    fn generate_arc_code(&self, value: &serde_json::Value, type_hint: &str) -> Result<String> {
        let inner_type = if type_hint.starts_with("Arc<") && type_hint.ends_with(">") {
            &type_hint[4..type_hint.len() - 1]
        } else {
            "serde_json::Value"
        };

        if let Some(inner) = value.get("__inner__") {
            let inner_code = self.generate_value_init_expr(inner, inner_type)?;
            Ok(format!("std::sync::Arc::new({})", inner_code))
        } else {
            // Fallback to serde_json::Value
            let json_str = serde_json::to_string(value)?;
            Ok(format!(
                "std::sync::Arc::new(serde_json::from_str::<serde_json::Value>(r#\"{}\"#).unwrap())",
                json_str
            ))
        }
    }

    /// Generate code for Rc<T> from __ferrumpy_kind__ metadata
    fn generate_rc_code(&self, value: &serde_json::Value, type_hint: &str) -> Result<String> {
        let inner_type = if type_hint.starts_with("Rc<") && type_hint.ends_with(">") {
            &type_hint[3..type_hint.len() - 1]
        } else {
            "serde_json::Value"
        };

        if let Some(inner) = value.get("__inner__") {
            let inner_code = self.generate_value_init_expr(inner, inner_type)?;
            Ok(format!("std::rc::Rc::new({})", inner_code))
        } else {
            let json_str = serde_json::to_string(value)?;
            Ok(format!(
                "std::rc::Rc::new(serde_json::from_str::<serde_json::Value>(r#\"{}\"#).unwrap())",
                json_str
            ))
        }
    }

    /// Generate code for Box<T> from __ferrumpy_kind__ metadata
    fn generate_box_code(&self, value: &serde_json::Value, type_hint: &str) -> Result<String> {
        let inner_type = if type_hint.starts_with("Box<") && type_hint.ends_with(">") {
            &type_hint[4..type_hint.len() - 1]
        } else {
            "serde_json::Value"
        };

        if let Some(inner) = value.get("__inner__") {
            let inner_code = self.generate_value_init_expr(inner, inner_type)?;
            Ok(format!("Box::new({})", inner_code))
        } else {
            let json_str = serde_json::to_string(value)?;
            Ok(format!(
                "Box::new(serde_json::from_str::<serde_json::Value>(r#\"{}\"#).unwrap())",
                json_str
            ))
        }
    }

    /// Generate code for user-defined enum from __ferrumpy_kind__: enum metadata
    fn generate_enum_code(&self, value: &serde_json::Value, type_hint: &str) -> Result<String> {
        let enum_type = value
            .get("__enum_type__")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        let variant = value
            .get("__variant__")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        let payload = value.get("__payload__");

        // Use the type_hint if it includes the full path, otherwise construct from enum_type
        let base_type = if type_hint.contains("::") {
            // Remove the variant part if present in type_hint
            if let Some(pos) = type_hint.rfind("::") {
                let before_last = &type_hint[..pos];
                if let Some(pos2) = before_last.rfind("::") {
                    // Check if last part looks like a variant (capitalized)
                    let last = &before_last[pos2 + 2..];
                    if !last.is_empty() && last.chars().next().unwrap().is_uppercase() {
                        before_last.to_string()
                    } else {
                        type_hint.to_string()
                    }
                } else {
                    before_last.to_string()
                }
            } else {
                type_hint.to_string()
            }
        } else {
            enum_type.to_string()
        };

        // Handle different payload types
        match payload {
            None | Some(serde_json::Value::Null) => {
                // Unit variant: Status::Active
                Ok(format!("{}::{}", base_type, variant))
            }
            Some(serde_json::Value::Array(arr)) => {
                // Tuple variant with multiple fields: Status::MultiValue(a, b, c)
                let mut parts = Vec::new();
                for elem in arr {
                    let elem_code = self.primitive_to_code(elem);
                    parts.push(elem_code);
                }
                Ok(format!("{}::{}({})", base_type, variant, parts.join(", ")))
            }
            Some(serde_json::Value::Object(obj)) => {
                // Struct variant: Status::Inactive { reason: "..." }
                let mut fields = Vec::new();
                for (key, val) in obj {
                    let val_code = self.primitive_to_code(val);
                    fields.push(format!("{}: {}", key, val_code));
                }
                Ok(format!(
                    "{}::{} {{ {} }}",
                    base_type,
                    variant,
                    fields.join(", ")
                ))
            }
            Some(single_val) => {
                // Tuple variant with single field: Status::Pending(42)
                let val_code = self.primitive_to_code(single_val);
                Ok(format!("{}::{}({})", base_type, variant, val_code))
            }
        }
    }

    /// Convert a JSON value to a Rust literal for enum payloads
    fn primitive_to_code(&self, val: &serde_json::Value) -> String {
        match val {
            serde_json::Value::Null => "()".to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => format!("{:?}.to_string()", s),
            serde_json::Value::Array(arr) => {
                let elements: Vec<String> = arr.iter().map(|e| self.primitive_to_code(e)).collect();
                format!("vec![{}]", elements.join(", "))
            }
            serde_json::Value::Object(_) => {
                // For complex objects, fall back to serde
                let json_str = serde_json::to_string(val).unwrap_or_default();
                format!(
                    "serde_json::from_str::<serde_json::Value>(r#\"{}\"#).unwrap()",
                    json_str
                )
            }
        }
    }

    /// Get type suffix for numeric literals (e.g., i32 -> "i32", f64 -> "f64")
    fn type_suffix(&self, type_hint: &str) -> &'static str {
        match type_hint {
            "i8" => "i8",
            "i16" => "i16",
            "i32" => "i32",
            "i64" => "i64",
            "i128" => "i128",
            "isize" => "isize",
            "u8" => "u8",
            "u16" => "u16",
            "u32" => "u32",
            "u64" => "u64",
            "u128" => "u128",
            "usize" => "usize",
            "f32" => "f32",
            "f64" => "f64",
            _ => "",
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

        // Skip smart pointers if they don't have a type hint or are malformed
        if type_hint.contains("Arc<") || type_hint.contains("Rc<") || type_hint.contains("Box<") {
            return type_hint.contains('<') && type_hint.ends_with('>');
        }

        // Skip RefCell/Cell (complex internal state, can't deserialize)
        if type_hint.contains("RefCell<") || type_hint.contains("Cell<") {
            return false;
        }

        // Skip allocator types (should be normalized away by Python, but double-check)
        if type_hint.contains("Global") || type_hint.contains("alloc::") {
            return false;
        }

        // NOW SUPPORTED: C-style arrays and Rust arrays via __ferrumpy_kind__ metadata
        // (Removed the array exclusion)

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
            // Allow __ferrumpy_kind__ metadata - but only if the type is actually supported
            if obj.contains_key("__ferrumpy_kind__") {
                // For types with references, we still can't deserialize
                // is_supported_type already checked for this, so trust the type_hint
                if type_hint.contains("&") {
                    return false; // Contains reference, can't restore
                }
                return true;
            }

            for key in obj.keys() {
                // These markers indicate serialization failed in Python
                if key.starts_with("__") && key.ends_with("__") {
                    return false;
                }
            }

            // For Result<T, E>, need {"Ok": ...} or {"Err": ...} format
            // (Legacy path - now handled by __ferrumpy_kind__)
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

    /// Get completions for the given source code at the specified position
    ///
    /// Returns a tuple of (completions, start_offset, end_offset) where:
    /// - completions: list of completion strings
    /// - start_offset: byte offset where the replacement should start
    /// - end_offset: byte offset where the replacement should end
    pub fn completions(
        &mut self,
        src: &str,
        position: usize,
    ) -> Result<(Vec<evcxr::Completion>, usize, usize)> {
        match self.context.completions(src, position) {
            Ok(completions) => Ok((
                completions.completions,
                completions.start_offset,
                completions.end_offset,
            )),
            Err(e) => Err(anyhow::anyhow!("Completion error: {:?}", e)),
        }
    }

    /// Check if a code fragment is complete, incomplete, or invalid
    pub fn fragment_validity(&self, source: &str) -> crate::repl::scan::FragmentValidity {
        crate::repl::scan::validate_source_fragment(source)
    }

    /// Interrupt any currently running evaluation by restarting the subprocess
    ///
    /// This is a forceful interruption that kills the subprocess and starts a new one.
    /// User-defined variables and functions will be lost, but LLDB snapshot variables
    /// are automatically restored.
    ///
    /// Returns Ok(()) if the interrupt was successful.
    pub fn interrupt(&mut self) -> Result<()> {
        eprintln!("[DEBUG] Interrupt called");

        // Execute :clear command which forces a subprocess restart
        // This effectively kills any running compilation or execution
        self.context
            .execute(":clear")
            .map_err(|e| anyhow::anyhow!("Failed to interrupt: {:?}", e))?;
        eprintln!("[DEBUG] :clear executed");

        // Restore snapshot if it was previously loaded
        if let (Some(json), Some(hints)) = (&self.snapshot_json, &self.snapshot_type_hints) {
            eprintln!(
                "[DEBUG] Found snapshot to restore: {} bytes JSON",
                json.len()
            );
            // Clone the data before calling load_snapshot (which needs &mut self)
            let json_clone = json.clone();
            let hints_clone = hints.clone();

            // Re-initialize the session state
            self.initialized = false;

            // Reload the snapshot to restore LLDB variables
            self.load_snapshot(&json_clone, &hints_clone).map_err(|e| {
                eprintln!("[DEBUG] Snapshot restoration failed: {:?}", e);
                anyhow::anyhow!("Failed to restore snapshot after interrupt: {:?}", e)
            })?;
            eprintln!("[DEBUG] Snapshot restored successfully");
        } else {
            eprintln!(
                "[DEBUG] No snapshot to restore (snapshot_json: {}, snapshot_type_hints: {})",
                self.snapshot_json.is_some(),
                self.snapshot_type_hints.is_some()
            );
        }

        Ok(())
    }

    /// Drain all pending stdout lines from the subprocess
    ///
    /// This prevents the stdout pipe from filling up and blocking the subprocess.
    /// Should be called periodically, especially after eval() operations.
    ///
    /// Returns a vector of output lines.
    pub fn drain_stdout(&mut self) -> Vec<String> {
        let mut output = Vec::new();
        while let Ok(line) = self.stdout.try_recv() {
            output.push(line);
        }
        output
    }

    /// Drain all pending stderr lines from the subprocess
    ///
    /// This prevents the stderr pipe from filling up and blocking the subprocess.
    /// Should be called periodically, especially after eval() operations.
    ///
    /// Returns a vector of error lines.
    pub fn drain_stderr(&mut self) -> Vec<String> {
        let mut output = Vec::new();
        while let Ok(line) = self.stderr.try_recv() {
            output.push(line);
        }
        output
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

    #[test]
    fn test_completions_api_structure() {
        match ReplSession::new() {
            Ok(mut session) => {
                // We don't necessarily need a full compilation for a basic check
                let result = session.completions("let x = ", 8);
                // Even if empty, it should be an Ok result with the right structure
                assert!(result.is_ok());
            }
            Err(_) => {}
        }
    }
}
