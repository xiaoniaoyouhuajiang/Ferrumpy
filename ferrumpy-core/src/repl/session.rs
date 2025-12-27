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

        Ok(Self {
            context,
            stdout: outputs.stdout,
            stderr: outputs.stderr,
            project_path: None,
            initialized: false,
        })
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

    /// Load variables from serialized JSON snapshot
    pub fn load_snapshot(&mut self, json_data: &str, _type_hints: &str) -> Result<String> {
        // First, add serde dependencies
        // Now that we use CommandContext, these :dep commands should work properly
        self.eval(":dep serde = { version = \"1\", features = [\"derive\"] }")?;
        self.eval(":dep serde_json = \"1\"")?;

        // Import serde
        self.eval("use serde::{Serialize, Deserialize};")?;

        // Parse the JSON snapshot
        let snapshot: serde_json::Value =
            serde_json::from_str(json_data).context("Failed to parse snapshot JSON")?;

        if let Some(variables) = snapshot.get("variables") {
            if let Some(vars) = variables.as_object() {
                for (name, value) in vars {
                    let json_str = serde_json::to_string(value)?;
                    // Use raw string literal to avoid escaping issues
                    let code = format!(
                        "let {} = serde_json::from_str::<serde_json::Value>(r#\"{}\"#).unwrap();",
                        name, json_str
                    );
                    self.eval(&code)?;
                }
            }
        }

        self.initialized = true;
        Ok("Snapshot loaded".to_string())
    }

    /// Evaluate a Rust expression
    pub fn eval(&mut self, code: &str) -> Result<String> {
        // Use CommandContext::execute instead of EvalContext::eval
        let outputs = self.context.execute(code).map_err(|e| match e {
            EvcxrError::CompilationErrors(errors) => {
                anyhow::anyhow!("Compilation errors: {:?}", errors)
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
