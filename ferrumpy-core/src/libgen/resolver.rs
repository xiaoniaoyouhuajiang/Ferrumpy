//! Module resolver
//!
//! Resolves `mod xxx;` declarations to find and read module files.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use syn::{parse_file, Item};

/// Resolve all module files referenced from a source file
pub fn resolve_modules(source_path: &Path) -> Result<HashMap<PathBuf, String>> {
    let mut modules = HashMap::new();
    let source_dir = source_path.parent().unwrap_or(Path::new("."));

    let source = std::fs::read_to_string(source_path)?;
    let ast = parse_file(&source)?;

    for item in &ast.items {
        if let Item::Mod(item_mod) = item {
            // Only process external modules (no content block)
            if item_mod.content.is_none() {
                resolve_module_recursive(source_dir, &item_mod.ident.to_string(), &mut modules)?;
            }
        }
    }

    Ok(modules)
}

fn resolve_module_recursive(
    base_dir: &Path,
    mod_name: &str,
    modules: &mut HashMap<PathBuf, String>,
) -> Result<()> {
    // Try to find the module file
    // Rust module resolution: mod foo; looks for foo.rs or foo/mod.rs
    let file_path = base_dir.join(format!("{}.rs", mod_name));
    let dir_path = base_dir.join(mod_name).join("mod.rs");

    let (actual_path, content) = if file_path.exists() {
        let content = std::fs::read_to_string(&file_path)?;
        (PathBuf::from(format!("{}.rs", mod_name)), content)
    } else if dir_path.exists() {
        let content = std::fs::read_to_string(&dir_path)?;
        (PathBuf::from(format!("{}/mod.rs", mod_name)), content)
    } else {
        // Module not found, skip
        eprintln!(
            "Warning: Module {} not found at {:?} or {:?}",
            mod_name, file_path, dir_path
        );
        return Ok(());
    };

    // Add to map
    modules.insert(actual_path.clone(), content.clone());

    // Parse and look for nested modules
    let ast = parse_file(&content)?;
    let new_base = if file_path.exists() {
        base_dir.join(mod_name)
    } else {
        base_dir.join(mod_name)
    };

    for item in &ast.items {
        if let Item::Mod(item_mod) = item {
            if item_mod.content.is_none() {
                // Recursively resolve nested modules
                let nested_name = item_mod.ident.to_string();
                resolve_module_recursive(&new_base, &nested_name, modules)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_simple_module() {
        // Create temp directory with module structure
        let temp = TempDir::new().unwrap();
        let src_dir = temp.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();

        // Create main.rs with mod declaration
        fs::write(
            src_dir.join("main.rs"),
            r#"
mod utils;
fn main() {}
"#,
        )
        .unwrap();

        // Create utils.rs
        fs::write(
            src_dir.join("utils.rs"),
            r#"
pub fn helper() -> i32 { 42 }
"#,
        )
        .unwrap();

        // Resolve modules
        let modules = resolve_modules(&src_dir.join("main.rs")).unwrap();

        assert_eq!(modules.len(), 1);
        assert!(modules.contains_key(&PathBuf::from("utils.rs")));
    }
}
