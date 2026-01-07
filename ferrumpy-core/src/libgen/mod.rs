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

    let mut cargo = String::new();
    cargo.push_str("[package]\n");
    cargo.push_str("name = \"ferrumpy_snapshot\"\n");
    cargo.push_str("version = \"0.1.3\"\n");
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

                match value {
                    toml::Value::String(version) => {
                        cargo.push_str(&format!("{} = \"{}\"\n", name, version));
                    }
                    toml::Value::Table(t) => {
                        // Handle complex dependencies - serialize as inline table
                        // We need to manually format as inline table since toml::to_string
                        // outputs multi-line format that breaks when used inline
                        let mut parts = Vec::new();
                        for (key, val) in t {
                            let val_str = match val {
                                toml::Value::String(s) => format!("\"{}\"", s),
                                toml::Value::Array(arr) => {
                                    let items: Vec<String> = arr
                                        .iter()
                                        .map(|v| match v {
                                            toml::Value::String(s) => format!("\"{}\"", s),
                                            _ => v.to_string(),
                                        })
                                        .collect();
                                    format!("[{}]", items.join(", "))
                                }
                                toml::Value::Boolean(b) => b.to_string(),
                                toml::Value::Integer(i) => i.to_string(),
                                toml::Value::Float(f) => f.to_string(),
                                _ => {
                                    // For nested tables, use toml serialization
                                    toml::to_string(val).unwrap_or_default().trim().to_string()
                                }
                            };
                            parts.push(format!("{} = {}", key, val_str));
                        }
                        cargo.push_str(&format!("{} = {{ {} }}\n", name, parts.join(", ")));
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(cargo)
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
}
