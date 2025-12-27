//! Python bindings for ferrumpy-core
//!
//! Provides pyo3 FFI interface for direct Python integration.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::expr::{parse_expr, Evaluator, Value};

/// Parse and evaluate a Rust expression
#[pyfunction]
fn eval_expression(
    py: Python<'_>,
    expr: &str,
    variables: &Bound<'_, PyDict>,
) -> PyResult<PyObject> {
    // Parse expression
    let ast =
        parse_expr(expr).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    // Build evaluator with variables
    let mut evaluator = Evaluator::new();

    for item in variables.items() {
        let tuple = item.downcast::<pyo3::types::PyTuple>()?;
        let name: String = tuple.get_item(0)?.extract()?;
        let value_item = tuple.get_item(1)?;
        let value_dict = value_item.downcast::<PyDict>()?;

        let type_name: String = value_dict
            .get_item("type")?
            .map(|v| v.extract())
            .transpose()?
            .unwrap_or_default();
        let value_str: String = value_dict
            .get_item("value")?
            .map(|v| v.extract())
            .transpose()?
            .unwrap_or_default();

        if let Some(val) = parse_value(&type_name, &value_str) {
            evaluator.set_variable(&name, val);
        }
    }

    // Evaluate
    match evaluator.eval(&ast) {
        Ok(value) => {
            let result = PyDict::new_bound(py);
            result.set_item("value", value.to_string())?;
            result.set_item("type", value.type_name())?;
            Ok(result.into())
        }
        Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
    }
}

/// Parse a variable value from string
fn parse_value(type_name: &str, value_str: &str) -> Option<Value> {
    let type_name = type_name.trim();
    let value_str = value_str.trim();

    match type_name {
        "i8" => value_str.parse().ok().map(Value::I8),
        "i16" => value_str.parse().ok().map(Value::I16),
        "i32" => value_str.parse().ok().map(Value::I32),
        "i64" => value_str.parse().ok().map(Value::I64),
        "i128" => value_str.parse().ok().map(Value::I128),
        "isize" => value_str.parse().ok().map(Value::Isize),
        "u8" => value_str.parse().ok().map(Value::U8),
        "u16" => value_str.parse().ok().map(Value::U16),
        "u32" => value_str.parse().ok().map(Value::U32),
        "u64" => value_str.parse().ok().map(Value::U64),
        "u128" => value_str.parse().ok().map(Value::U128),
        "usize" => value_str.parse().ok().map(Value::Usize),
        "f32" => value_str.parse().ok().map(Value::F32),
        "f64" => value_str.parse().ok().map(Value::F64),
        "bool" => value_str.parse().ok().map(Value::Bool),
        _ => None,
    }
}

/// Parse a Rust expression and return AST as JSON
#[pyfunction]
fn parse_expression(expr: &str) -> PyResult<String> {
    let ast =
        parse_expr(expr).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    serde_json::to_string(&ast)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

/// Python wrapper for ReplSession
#[pyclass]
struct PyReplSession {
    inner: Option<crate::repl::ReplSession>,
}

#[pymethods]
impl PyReplSession {
    /// Create a new REPL session
    #[new]
    fn new() -> PyResult<Self> {
        // Note: evcxr requires runtime_hook() to be called first
        // We'll try to create the session and handle errors gracefully
        match crate::repl::ReplSession::new() {
            Ok(session) => Ok(Self {
                inner: Some(session),
            }),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Failed to create REPL session: {}",
                e
            ))),
        }
    }

    /// Evaluate a Rust expression
    fn eval(&mut self, code: &str) -> PyResult<String> {
        let session = self
            .inner
            .as_mut()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Session not initialized"))?;

        session
            .eval(code)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Add a crate dependency
    fn add_dep(&mut self, name: &str, spec: &str) -> PyResult<String> {
        let session = self
            .inner
            .as_mut()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Session not initialized"))?;

        session
            .add_dep(name, spec)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Load variables from JSON snapshot
    fn load_snapshot(&mut self, json_data: &str, type_hints: &str) -> PyResult<String> {
        let session = self
            .inner
            .as_mut()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Session not initialized"))?;

        session
            .load_snapshot(json_data, type_hints)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Check if session is initialized
    fn is_initialized(&self) -> bool {
        self.inner
            .as_ref()
            .map(|s| s.is_initialized())
            .unwrap_or(false)
    }

    /// Get any stderr output
    fn get_stderr(&self) -> Vec<String> {
        self.inner
            .as_ref()
            .map(|s| s.get_stderr())
            .unwrap_or_default()
    }

    /// Add a path dependency (for user's lib crate)
    fn add_path_dep(&mut self, name: &str, path: &str) -> PyResult<String> {
        let session = self
            .inner
            .as_mut()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Session not initialized"))?;

        session
            .add_path_dep(name, std::path::Path::new(path))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}

/// Generate a companion lib crate from a user's project
///
/// Args:
///     project_path: Path to the user's Rust project (containing Cargo.toml)
///     output_dir: Optional output directory (None = use temp dir)
///
/// Returns:
///     Tuple of (lib_path, crate_name)
#[pyfunction]
fn generate_lib(project_path: &str, output_dir: Option<&str>) -> PyResult<(String, String)> {
    use crate::libgen::{generate_lib as rust_generate_lib, LibGenConfig};

    let config = LibGenConfig {
        add_serde_derives: true,
        output_dir: output_dir.map(std::path::PathBuf::from),
    };

    let result = rust_generate_lib(std::path::Path::new(project_path), config)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    Ok((result.path.to_string_lossy().to_string(), result.crate_name))
}

/// FerrumPy Python module
#[pymodule]
fn ferrumpy_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(eval_expression, m)?)?;
    m.add_function(wrap_pyfunction!(parse_expression, m)?)?;
    m.add_function(wrap_pyfunction!(generate_lib, m)?)?;
    m.add_class::<PyReplSession>()?;
    Ok(())
}
