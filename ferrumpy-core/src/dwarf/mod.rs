//! DWARF type processing module
//!
//! Converts DWARF type names to Rust syntax and handles type layout information.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DwarfError {
    #[error("Failed to parse type name: {0}")]
    ParseError(String),
}

/// Convert DWARF type name to Rust syntax
///
/// Examples:
/// - `alloc::string::String` -> `String`
/// - `alloc::vec::Vec<i32>` -> `Vec<i32>`
/// - `core::option::Option<alloc::string::String>` -> `Option<String>`
pub fn dwarf_type_to_rust(dwarf_name: &str) -> Result<String, DwarfError> {
    let mut result = dwarf_name.to_string();

    // Standard library path replacements
    let replacements = [
        ("alloc::string::", ""),
        ("alloc::vec::", ""),
        ("alloc::boxed::", ""),
        ("alloc::sync::", ""),
        ("alloc::rc::", ""),
        ("alloc::borrow::", ""),
        ("alloc::collections::", ""),
        ("core::option::", ""),
        ("core::result::", ""),
        ("core::cell::", ""),
        ("std::collections::", ""),
        ("std::sync::", ""),
    ];

    for (from, to) in replacements {
        result = result.replace(from, to);
    }

    // Remove hash suffixes (e.g., ::h1a2b3c4d)
    if let Some(pos) = result.find("::h") {
        if result[pos + 3..].chars().all(|c| c.is_ascii_hexdigit()) {
            result = result[..pos].to_string();
        }
    }

    Ok(result)
}

/// Information about a local variable extracted from debug info
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VariableInfo {
    pub name: String,
    pub type_name: String,
    pub rust_type: String,
    /// String representation of the value (for primitive types)
    #[serde(default)]
    pub value: String,
}

impl VariableInfo {
    pub fn new(name: String, type_name: String) -> Result<Self, DwarfError> {
        let rust_type = dwarf_type_to_rust(&type_name)?;
        Ok(Self {
            name,
            type_name,
            rust_type,
            value: String::new(),
        })
    }

    pub fn with_value(name: String, type_name: String, value: String) -> Result<Self, DwarfError> {
        let rust_type = dwarf_type_to_rust(&type_name)?;
        Ok(Self {
            name,
            type_name,
            rust_type,
            value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dwarf_type_to_rust() {
        assert_eq!(
            dwarf_type_to_rust("alloc::string::String").unwrap(),
            "String"
        );
        assert_eq!(
            dwarf_type_to_rust("alloc::vec::Vec<i32>").unwrap(),
            "Vec<i32>"
        );
        assert_eq!(
            dwarf_type_to_rust("core::option::Option<alloc::string::String>").unwrap(),
            "Option<String>"
        );
        assert_eq!(
            dwarf_type_to_rust("core::result::Result<i32, alloc::string::String>").unwrap(),
            "Result<i32, String>"
        );
    }
}
