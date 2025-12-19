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

/// FerrumPy Python module
#[pymodule]
fn ferrumpy_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(eval_expression, m)?)?;
    m.add_function(wrap_pyfunction!(parse_expression, m)?)?;
    Ok(())
}
