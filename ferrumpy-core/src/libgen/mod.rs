//! Auto-Lib Generator
//!
//! Transforms a user's main.rs project into a lib crate that can be
//! depended upon by the REPL environment.

mod resolver;
mod transformer;

pub use resolver::resolve_modules;
pub use transformer::transform_to_lib;

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration for lib generation
pub struct LibGenConfig {
    /// Add serde derives to structs/enums
    pub add_serde_derives: bool,
    /// Output directory (None = create temp dir)
    pub output_dir: Option<PathBuf>,
}

impl Default for LibGenConfig {
    fn default() -> Self {
        Self {
            add_serde_derives: true,
            output_dir: None,
        }
    }
}

/// Result of lib generation
pub struct GeneratedLib {
    /// Path to the generated lib crate
    pub path: PathBuf,
    /// Crate name (for use in dependencies)
    pub crate_name: String,
}

/// Generate a lib crate from a user's project
pub fn generate_lib(project_path: &Path, config: LibGenConfig) -> Result<GeneratedLib> {
    // 1. Create output directory
    let output_dir = config.output_dir.unwrap_or_else(|| {
        let tmp = std::env::temp_dir().join(format!("ferrumpy_lib_{}", std::process::id()));
        tmp
    });
    fs::create_dir_all(&output_dir)?;
    fs::create_dir_all(output_dir.join("src"))?;

    // 2. Determine source file (main.rs or lib.rs)
    let main_rs = project_path.join("src/main.rs");
    let lib_rs = project_path.join("src/lib.rs");

    let (source_file, is_bin) = if main_rs.exists() {
        (main_rs, true)
    } else if lib_rs.exists() {
        (lib_rs, false)
    } else {
        anyhow::bail!("No src/main.rs or src/lib.rs found in project");
    };

    // 3. Transform main source file
    let transformed =
        transformer::transform_to_lib(&source_file, is_bin, config.add_serde_derives)?;
    fs::write(output_dir.join("src/lib.rs"), &transformed)?;

    // 4. Resolve and copy module files
    let modules = resolver::resolve_modules(&source_file)?;
    for (rel_path, content) in modules {
        let dest = output_dir.join("src").join(&rel_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let transformed_mod = transformer::transform_module(&content, config.add_serde_derives)?;
        fs::write(&dest, transformed_mod)?;
    }

    // 5. Generate Cargo.toml
    let cargo_toml = generate_cargo_toml(project_path, config.add_serde_derives)?;
    fs::write(output_dir.join("Cargo.toml"), cargo_toml)?;

    Ok(GeneratedLib {
        path: output_dir,
        crate_name: "ferrumpy_snapshot".to_string(),
    })
}

fn generate_cargo_toml(project_path: &Path, add_serde: bool) -> Result<String> {
    let user_cargo = project_path.join("Cargo.toml");
    let user_content = fs::read_to_string(&user_cargo)?;

    // Parse and extract dependencies
    let user_toml: toml::Value = user_content.parse()?;

    // Try to find workspace root and load workspace dependencies
    let workspace_deps = find_workspace_dependencies(project_path);

    let mut cargo = String::new();
    cargo.push_str("[package]\n");
    cargo.push_str("name = \"ferrumpy_snapshot\"\n");
    cargo.push_str("version = \"0.1.4\"\n");
    cargo.push_str("edition = \"2021\"\n\n");

    cargo.push_str("[lib]\n");
    cargo.push_str("crate-type = [\"rlib\"]\n\n");

    cargo.push_str("[dependencies]\n");

    // Add serde if requested
    if add_serde {
        cargo.push_str("serde = { version = \"1\", features = [\"derive\"] }\n");
        cargo.push_str("serde_json = \"1\"\n");
    }

    // Copy user dependencies
    if let Some(deps) = user_toml.get("dependencies") {
        if let Some(table) = deps.as_table() {
            for (name, value) in table {
                // Skip if we already added serde
                if add_serde && (name == "serde" || name == "serde_json") {
                    continue;
                }

                // Check if this is a workspace dependency
                if let Some(resolved) =
                    resolve_dependency(name, value, &workspace_deps, project_path)
                {
                    cargo.push_str(&resolved);
                    cargo.push('\n');
                }
            }
        }
    }

    Ok(cargo)
}

/// Find workspace root and extract workspace.dependencies
fn find_workspace_dependencies(project_path: &Path) -> Option<toml::value::Table> {
    // Walk up from project_path to find workspace root (contains [workspace] section)
    let mut current = project_path.to_path_buf();

    for _ in 0..10 {
        // limit search depth
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = fs::read_to_string(&cargo_toml) {
                if let Ok(parsed) = content.parse::<toml::Value>() {
                    // Check if this is a workspace root
                    if let Some(workspace) = parsed.get("workspace") {
                        if let Some(deps) = workspace.get("dependencies") {
                            if let Some(table) = deps.as_table() {
                                return Some(table.clone());
                            }
                        }
                    }
                }
            }
        }

        // Move up to parent directory
        if !current.pop() {
            break;
        }
    }

    None
}

/// Resolve a dependency, handling workspace = true and path = "..." cases
fn resolve_dependency(
    name: &str,
    value: &toml::Value,
    workspace_deps: &Option<toml::value::Table>,
    project_path: &Path,
) -> Option<String> {
    match value {
        toml::Value::String(version) => Some(format!("{} = \"{}\"", name, version)),
        toml::Value::Table(t) => {
            // Check if this is a workspace dependency
            if t.get("workspace").and_then(|v| v.as_bool()) == Some(true) {
                // Try to resolve from workspace dependencies
                if let Some(ws_deps) = workspace_deps {
                    if let Some(ws_dep) = ws_deps.get(name) {
                        // Recursively resolve (in case workspace dep is also a table)
                        return resolve_dependency(name, ws_dep, &None, project_path);
                    }
                }
                // If we can't resolve, skip this dependency with a warning
                eprintln!(
                    "[FerrumPy] Warning: Skipping workspace dependency '{}' (could not resolve from workspace root)",
                    name
                );
                return None;
            }

            // Check if this is a path dependency - need to convert relative to absolute
            if let Some(path_val) = t.get("path") {
                if let Some(path_str) = path_val.as_str() {
                    let dep_path = Path::new(path_str);
                    let absolute_path = if dep_path.is_relative() {
                        project_path.join(dep_path)
                    } else {
                        dep_path.to_path_buf()
                    };

                    // Convert to absolute path and rebuild the dependency
                    let mut parts = Vec::new();
                    parts.push(format!("path = \"{}\"", absolute_path.display()));

                    // Copy other keys (version, features, etc.)
                    for (key, val) in t {
                        if key == "path" {
                            continue; // Already handled
                        }
                        let val_str = format_toml_value(val);
                        parts.push(format!("{} = {}", key, val_str));
                    }

                    return Some(format!("{} = {{ {} }}", name, parts.join(", ")));
                }
            }

            // Handle complex dependencies - serialize as inline table
            let mut parts = Vec::new();
            for (key, val) in t {
                // Skip 'workspace' key if present
                if key == "workspace" {
                    continue;
                }
                let val_str = format_toml_value(val);
                parts.push(format!("{} = {}", key, val_str));
            }

            if parts.is_empty() {
                None
            } else {
                Some(format!("{} = {{ {} }}", name, parts.join(", ")))
            }
        }
        _ => None,
    }
}

/// Format a TOML value for inline use
fn format_toml_value(val: &toml::Value) -> String {
    match val {
        toml::Value::String(s) => format!("\"{}\"", s),
        toml::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_toml_value).collect();
            format!("[{}]", items.join(", "))
        }
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Table(t) => {
            let parts: Vec<String> = t
                .iter()
                .map(|(k, v)| format!("{} = {}", k, format_toml_value(v)))
                .collect();
            format!("{{ {} }}", parts.join(", "))
        }
        _ => toml::to_string(val).unwrap_or_default().trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_lib_config_default() {
        let config = LibGenConfig::default();
        assert!(config.add_serde_derives);
        assert!(config.output_dir.is_none());
    }

    #[test]
    fn test_format_toml_value_string() {
        let val = toml::Value::String("1.0".to_string());
        assert_eq!(format_toml_value(&val), "\"1.0\"");
    }

    #[test]
    fn test_format_toml_value_array() {
        let val = toml::Value::Array(vec![
            toml::Value::String("derive".to_string()),
            toml::Value::String("serde".to_string()),
        ]);
        assert_eq!(format_toml_value(&val), "[\"derive\", \"serde\"]");
    }

    #[test]
    fn test_format_toml_value_bool() {
        let val = toml::Value::Boolean(true);
        assert_eq!(format_toml_value(&val), "true");
    }

    #[test]
    fn test_resolve_dependency_simple_version() {
        let val = toml::Value::String("1.0".to_string());
        let dummy_path = Path::new("/tmp/test");
        let result = resolve_dependency("serde", &val, &None, dummy_path);
        assert_eq!(result, Some("serde = \"1.0\"".to_string()));
    }

    #[test]
    fn test_resolve_dependency_with_features() {
        let mut table = toml::value::Table::new();
        table.insert(
            "version".to_string(),
            toml::Value::String("1.0".to_string()),
        );
        table.insert(
            "features".to_string(),
            toml::Value::Array(vec![toml::Value::String("derive".to_string())]),
        );
        let val = toml::Value::Table(table);
        let dummy_path = Path::new("/tmp/test");
        let result = resolve_dependency("serde", &val, &None, dummy_path).unwrap();
        // Order may vary, so check both possibilities
        assert!(
            result.contains("version = \"1.0\"") && result.contains("features = [\"derive\"]"),
            "Got: {}",
            result
        );
    }

    #[test]
    fn test_resolve_dependency_workspace_true_with_resolution() {
        // Simulate { workspace = true }
        let mut dep_table = toml::value::Table::new();
        dep_table.insert("workspace".to_string(), toml::Value::Boolean(true));
        let dep_val = toml::Value::Table(dep_table);

        // Simulate workspace.dependencies.bitflags = "2.4"
        let mut ws_deps = toml::value::Table::new();
        ws_deps.insert(
            "bitflags".to_string(),
            toml::Value::String("2.4".to_string()),
        );

        let dummy_path = Path::new("/tmp/test");
        let result = resolve_dependency("bitflags", &dep_val, &Some(ws_deps), dummy_path);
        assert_eq!(result, Some("bitflags = \"2.4\"".to_string()));
    }

    #[test]
    fn test_resolve_dependency_workspace_true_with_complex_resolution() {
        // Simulate { workspace = true }
        let mut dep_table = toml::value::Table::new();
        dep_table.insert("workspace".to_string(), toml::Value::Boolean(true));
        let dep_val = toml::Value::Table(dep_table);

        // Simulate workspace.dependencies.tokio = { version = "1", features = ["full"] }
        let mut tokio_table = toml::value::Table::new();
        tokio_table.insert("version".to_string(), toml::Value::String("1".to_string()));
        tokio_table.insert(
            "features".to_string(),
            toml::Value::Array(vec![toml::Value::String("full".to_string())]),
        );
        let mut ws_deps = toml::value::Table::new();
        ws_deps.insert("tokio".to_string(), toml::Value::Table(tokio_table));

        let dummy_path = Path::new("/tmp/test");
        let result = resolve_dependency("tokio", &dep_val, &Some(ws_deps), dummy_path).unwrap();
        assert!(result.contains("version = \"1\""), "Got: {}", result);
        assert!(result.contains("features = [\"full\"]"), "Got: {}", result);
    }

    #[test]
    fn test_resolve_dependency_workspace_true_without_resolution() {
        // Simulate { workspace = true } but no workspace deps available
        let mut dep_table = toml::value::Table::new();
        dep_table.insert("workspace".to_string(), toml::Value::Boolean(true));
        let dep_val = toml::Value::Table(dep_table);

        let dummy_path = Path::new("/tmp/test");
        let result = resolve_dependency("unknown_dep", &dep_val, &None, dummy_path);
        assert_eq!(result, None); // Should skip with warning
    }

    #[test]
    fn test_resolve_dependency_workspace_true_dep_not_in_workspace() {
        // Simulate { workspace = true } but dep not in workspace.dependencies
        let mut dep_table = toml::value::Table::new();
        dep_table.insert("workspace".to_string(), toml::Value::Boolean(true));
        let dep_val = toml::Value::Table(dep_table);

        let ws_deps = toml::value::Table::new(); // Empty

        let dummy_path = Path::new("/tmp/test");
        let result = resolve_dependency("missing_dep", &dep_val, &Some(ws_deps), dummy_path);
        assert_eq!(result, None); // Should skip with warning
    }

    #[test]
    fn test_resolve_dependency_path_relative() {
        // Simulate { path = "crates/other_crate" }
        let mut dep_table = toml::value::Table::new();
        dep_table.insert(
            "path".to_string(),
            toml::Value::String("crates/other_crate".to_string()),
        );
        let dep_val = toml::Value::Table(dep_table);

        let project_path = Path::new("/home/user/myproject");
        let result = resolve_dependency("other_crate", &dep_val, &None, project_path).unwrap();

        // Should convert relative path to absolute
        assert!(
            result.contains("path = \"/home/user/myproject/crates/other_crate\""),
            "Got: {}",
            result
        );
    }

    #[test]
    fn test_resolve_dependency_path_with_features() {
        // Simulate { path = "crates/my_lib", features = ["async"] }
        let mut dep_table = toml::value::Table::new();
        dep_table.insert(
            "path".to_string(),
            toml::Value::String("crates/my_lib".to_string()),
        );
        dep_table.insert(
            "features".to_string(),
            toml::Value::Array(vec![toml::Value::String("async".to_string())]),
        );
        let dep_val = toml::Value::Table(dep_table);

        let project_path = Path::new("/workspace/project");
        let result = resolve_dependency("my_lib", &dep_val, &None, project_path).unwrap();

        assert!(
            result.contains("path = \"/workspace/project/crates/my_lib\""),
            "Got: {}",
            result
        );
        assert!(result.contains("features = [\"async\"]"), "Got: {}", result);
    }
}
