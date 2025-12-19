//! Value types for expression evaluation
//!
//! Represents the result of evaluating an expression.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Runtime value with strict Rust typing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    // Signed integers
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Isize(isize),
    
    // Unsigned integers
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),
    
    // Floating point
    F32(f32),
    F64(f64),
    
    // Other primitives
    Bool(bool),
    Char(char),
    
    // String types
    String(String),
    
    // Unit
    Unit,
    
    // Reference to complex type (handle to SBValue)
    Ref {
        address: u64,
        type_name: String,
    },
}

impl Value {
    /// Get the type name of this value
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::I8(_) => "i8",
            Value::I16(_) => "i16",
            Value::I32(_) => "i32",
            Value::I64(_) => "i64",
            Value::I128(_) => "i128",
            Value::Isize(_) => "isize",
            Value::U8(_) => "u8",
            Value::U16(_) => "u16",
            Value::U32(_) => "u32",
            Value::U64(_) => "u64",
            Value::U128(_) => "u128",
            Value::Usize(_) => "usize",
            Value::F32(_) => "f32",
            Value::F64(_) => "f64",
            Value::Bool(_) => "bool",
            Value::Char(_) => "char",
            Value::String(_) => "String",
            Value::Unit => "()",
            Value::Ref { .. } => "ref",
        }
    }
    
    /// Check if this is a numeric type
    pub fn is_numeric(&self) -> bool {
        matches!(self, 
            Value::I8(_) | Value::I16(_) | Value::I32(_) | Value::I64(_) | Value::I128(_) | Value::Isize(_) |
            Value::U8(_) | Value::U16(_) | Value::U32(_) | Value::U64(_) | Value::U128(_) | Value::Usize(_) |
            Value::F32(_) | Value::F64(_)
        )
    }
    
    /// Check if this is an integer type
    pub fn is_integer(&self) -> bool {
        matches!(self,
            Value::I8(_) | Value::I16(_) | Value::I32(_) | Value::I64(_) | Value::I128(_) | Value::Isize(_) |
            Value::U8(_) | Value::U16(_) | Value::U32(_) | Value::U64(_) | Value::U128(_) | Value::Usize(_)
        )
    }
    
    /// Check if this is a signed integer
    pub fn is_signed(&self) -> bool {
        matches!(self,
            Value::I8(_) | Value::I16(_) | Value::I32(_) | Value::I64(_) | Value::I128(_) | Value::Isize(_)
        )
    }
    
    /// Convert to i128 if integer
    pub fn to_i128(&self) -> Option<i128> {
        match self {
            Value::I8(v) => Some(*v as i128),
            Value::I16(v) => Some(*v as i128),
            Value::I32(v) => Some(*v as i128),
            Value::I64(v) => Some(*v as i128),
            Value::I128(v) => Some(*v),
            Value::Isize(v) => Some(*v as i128),
            Value::U8(v) => Some(*v as i128),
            Value::U16(v) => Some(*v as i128),
            Value::U32(v) => Some(*v as i128),
            Value::U64(v) => Some(*v as i128),
            Value::U128(v) => i128::try_from(*v).ok(),
            Value::Usize(v) => Some(*v as i128),
            _ => None,
        }
    }
    
    /// Convert to f64 if floating point
    pub fn to_f64(&self) -> Option<f64> {
        match self {
            Value::F32(v) => Some(*v as f64),
            Value::F64(v) => Some(*v),
            _ => None,
        }
    }
    
    /// Convert to bool
    pub fn to_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::I8(v) => write!(f, "{}", v),
            Value::I16(v) => write!(f, "{}", v),
            Value::I32(v) => write!(f, "{}", v),
            Value::I64(v) => write!(f, "{}", v),
            Value::I128(v) => write!(f, "{}", v),
            Value::Isize(v) => write!(f, "{}", v),
            Value::U8(v) => write!(f, "{}", v),
            Value::U16(v) => write!(f, "{}", v),
            Value::U32(v) => write!(f, "{}", v),
            Value::U64(v) => write!(f, "{}", v),
            Value::U128(v) => write!(f, "{}", v),
            Value::Usize(v) => write!(f, "{}", v),
            Value::F32(v) => write!(f, "{}", v),
            Value::F64(v) => write!(f, "{}", v),
            Value::Bool(v) => write!(f, "{}", v),
            Value::Char(v) => write!(f, "'{}'", v),
            Value::String(v) => write!(f, "\"{}\"", v),
            Value::Unit => write!(f, "()"),
            Value::Ref { type_name, address } => write!(f, "&{} @ 0x{:x}", type_name, address),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_value_type_names() {
        assert_eq!(Value::I32(42).type_name(), "i32");
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::String("hello".to_string()).type_name(), "String");
    }
    
    #[test]
    fn test_value_display() {
        assert_eq!(format!("{}", Value::I32(42)), "42");
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(format!("{}", Value::String("hello".to_string())), "\"hello\"");
    }
}
