//! Expression error types

use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum EvalError {
    // Parse errors
    #[error("Parse error: {message}")]
    ParseError { message: String },
    
    // Semantic errors
    #[error("Unsupported expression: {kind}. This feature is not yet implemented.")]
    UnsupportedExpression { kind: String },
    
    #[error("Unknown variable: '{name}'")]
    UnknownVariable { name: String },
    
    #[error("Type mismatch: expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    
    #[error("Cannot apply operator '{op}' to types {left} and {right}")]
    InvalidOperation { op: String, left: String, right: String },
    
    // Runtime errors
    #[error("Division by zero")]
    DivisionByZero,
    
    #[error("Index out of bounds: index {index}, length {length}")]
    IndexOutOfBounds { index: usize, length: usize },
    
    #[error("Null pointer dereference")]
    NullPointer,
    
    #[error("Field '{field}' not found on type {type_name}")]
    FieldNotFound { field: String, type_name: String },
    
    #[error("Internal error: {0}")]
    Internal(String),
}

impl EvalError {
    pub fn unsupported(kind: impl Into<String>) -> Self {
        EvalError::UnsupportedExpression { kind: kind.into() }
    }
    
    pub fn unknown_var(name: impl Into<String>) -> Self {
        EvalError::UnknownVariable { name: name.into() }
    }
    
    pub fn type_mismatch(expected: impl Into<String>, found: impl Into<String>) -> Self {
        EvalError::TypeMismatch { 
            expected: expected.into(), 
            found: found.into() 
        }
    }
}
