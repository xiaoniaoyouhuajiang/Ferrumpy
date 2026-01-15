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

    // 5. Generate Cargo.toml (with path dependency resolution)
    let (cargo_toml, path_deps) =
        generate_cargo_toml(project_path, &output_dir, config.add_serde_derives)?;
    fs::write(output_dir.join("Cargo.toml"), cargo_toml)?;

    // 6. Add pub use statements for path dependencies to lib.rs
    // This makes types from those crates accessible by simple names
    let mut lib_content = transformed;
    if !path_deps.is_empty() {
        lib_content.push_str("\n// Re-export types from path dependencies\n");
        for dep in &path_deps {
            // Convert hyphens to underscores for valid Rust identifiers
            let crate_name = dep.replace('-', "_");
            lib_content.push_str(&format!("pub use {}::*;\n", crate_name));
        }
    }
    fs::write(output_dir.join("src/lib.rs"), &lib_content)?;

    Ok(GeneratedLib {
        path: output_dir,
        crate_name: "ferrumpy_snapshot".to_string(),
    })
}

/// Returns (cargo_toml_content, path_dependency_names)
fn generate_cargo_toml(
    project_path: &Path,
    output_dir: &Path,
    add_serde: bool,
) -> Result<(String, Vec<String>)> {
    let user_cargo = project_path.join("Cargo.toml");
    let user_content = fs::read_to_string(&user_cargo)?;

    // Parse and extract dependencies
    let user_toml: toml::Value = user_content.parse()?;

    // Try to find workspace root and load workspace dependencies
    let (workspace_deps, workspace_root) = find_workspace_dependencies(project_path);

    // For path resolution: use workspace_root if available, otherwise project_path
    let path_base = workspace_root.as_deref().unwrap_or(project_path);

    if std::env::var("FERRUMPY_DEBUG").is_ok() {
        eprintln!("[libgen] project_path: {:?}", project_path);
        eprintln!("[libgen] workspace_root: {:?}", workspace_root);
        eprintln!("[libgen] path_base: {:?}", path_base);
        eprintln!(
            "[libgen] workspace_deps: {:?}",
            workspace_deps
                .as_ref()
                .map(|d| d.keys().collect::<Vec<_>>())
        );
    }

    let mut cargo = String::new();
    cargo.push_str("[package]\n");
    cargo.push_str("name = \"ferrumpy_snapshot\"\n");
    cargo.push_str("version = \"0.1.5\"\n");
    cargo.push_str("edition = \"2021\"\n\n");

    cargo.push_str("[lib]\n");
    cargo.push_str("crate-type = [\"rlib\"]\n\n");

    cargo.push_str("[dependencies]\n");

    // Add serde if requested
    if add_serde {
        cargo.push_str("serde = { version = \"1\", features = [\"derive\"] }\n");
        cargo.push_str("serde_json = \"1\"\n");
    }

    // Track path dependencies for re-export
    let mut path_deps: Vec<String> = Vec::new();

    // Copy user dependencies
    if let Some(deps) = user_toml.get("dependencies") {
        if let Some(table) = deps.as_table() {
            for (name, value) in table {
                // Skip if we already added serde
                if add_serde && (name == "serde" || name == "serde_json") {
                    continue;
                }

                // Check if this is a path dependency (directly or via workspace)
                let is_path_dep = is_path_dependency(value, &workspace_deps);

                // Resolve dependency (handles workspace deps and path deps)
                if let Some(resolved) =
                    resolve_dependency(name, value, &workspace_deps, path_base, output_dir)
                {
                    cargo.push_str(&resolved);
                    cargo.push('\n');

                    // Track path deps for re-export
                    if is_path_dep {
                        path_deps.push(name.clone());
                    }
                }
            }
        }
    }

    Ok((cargo, path_deps))
}

/// Check if a dependency is a path dependency (directly or via workspace)
fn is_path_dependency(value: &toml::Value, workspace_deps: &Option<toml::value::Table>) -> bool {
    match value {
        toml::Value::Table(t) => {
            // Direct path dependency
            if t.get("path").is_some() {
                return true;
            }
            // Workspace dependency - check if it resolves to path
            if t.get("workspace").and_then(|v| v.as_bool()) == Some(true) {
                if let Some(ws_deps) = workspace_deps {
                    // Check all keys since we don't know the name here
                    for (_, ws_val) in ws_deps {
                        if let toml::Value::Table(ws_t) = ws_val {
                            if ws_t.get("path").is_some() {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Find workspace root and extract workspace.dependencies
/// Returns (workspace_deps, workspace_root_path)
fn find_workspace_dependencies(
    project_path: &Path,
) -> (Option<toml::value::Table>, Option<PathBuf>) {
    // Walk up from project_path to find workspace root (contains [workspace] section)
    let mut current = project_path.to_path_buf();

    if std::env::var("FERRUMPY_DEBUG").is_ok() {
        eprintln!(
            "[libgen] Looking for workspace root starting from: {:?}",
            project_path
        );
    }

    for _ in 0..10 {
        // limit search depth
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = fs::read_to_string(&cargo_toml) {
                if let Ok(parsed) = content.parse::<toml::Value>() {
                    // Check if this is a workspace root
                    if let Some(workspace) = parsed.get("workspace") {
                        if std::env::var("FERRUMPY_DEBUG").is_ok() {
                            eprintln!("[libgen] Found workspace root at: {:?}", current);
                        }
                        if let Some(deps) = workspace.get("dependencies") {
                            if let Some(table) = deps.as_table() {
                                return (Some(table.clone()), Some(current));
                            }
                        }
                        // Workspace exists but no dependencies section
                        return (None, Some(current));
                    }
                }
            }
        }

        // Move up to parent directory
        if !current.pop() {
            break;
        }
    }

    if std::env::var("FERRUMPY_DEBUG").is_ok() {
        eprintln!("[libgen] No workspace root found");
    }

    (None, None)
}

/// Resolve a dependency, handling workspace = true and path = "..." cases
/// For path deps with workspace deps, creates a resolved copy in output_dir/deps/
fn resolve_dependency(
    name: &str,
    value: &toml::Value,
    workspace_deps: &Option<toml::value::Table>,
    path_base: &Path,
    output_dir: &Path,
) -> Option<String> {
    match value {
        toml::Value::String(version) => Some(format!("{} = \"{}\"", name, version)),
        toml::Value::Table(t) => {
            // Check if this is a workspace dependency
            if t.get("workspace").and_then(|v| v.as_bool()) == Some(true) {
                if std::env::var("FERRUMPY_DEBUG").is_ok() {
                    eprintln!(
                        "[libgen] Resolving workspace dep '{}', available ws_deps: {:?}",
                        name,
                        workspace_deps
                            .as_ref()
                            .map(|d| d.keys().collect::<Vec<_>>())
                    );
                }
                // Try to resolve from workspace dependencies
                if let Some(ws_deps) = workspace_deps {
                    if let Some(ws_dep) = ws_deps.get(name) {
                        // Recursively resolve (in case workspace dep is also a table)
                        return resolve_dependency(
                            name,
                            ws_dep,
                            workspace_deps,
                            path_base,
                            output_dir,
                        );
                    }
                }
                // If we can't resolve, skip this dependency with a warning
                eprintln!(
                    "[FerrumPy] Warning: Skipping workspace dependency '{}' (could not resolve from workspace root)",
                    name
                );
                return None;
            }

            // Check if this is a path dependency
            if let Some(path_val) = t.get("path") {
                if let Some(path_str) = path_val.as_str() {
                    let dep_path = Path::new(path_str);
                    let absolute_path = if dep_path.is_relative() {
                        path_base.join(dep_path)
                    } else {
                        dep_path.to_path_buf()
                    };

                    // Check if this crate uses workspace dependencies
                    let dep_cargo_toml = absolute_path.join("Cargo.toml");
                    let has_workspace_deps = if dep_cargo_toml.exists() {
                        if let Ok(content) = fs::read_to_string(&dep_cargo_toml) {
                            content.contains("workspace = true")
                                || content.contains(".workspace = true")
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if has_workspace_deps {
                        // Create a resolved copy of the path dependency
                        if let Some(resolved_path) = create_resolved_path_dep(
                            name,
                            &absolute_path,
                            workspace_deps,
                            path_base,
                            output_dir,
                        ) {
                            let mut parts = Vec::new();
                            parts.push(format!("path = \"{}\"", resolved_path.display()));

                            // Copy other keys (version, features, etc.)
                            for (key, val) in t {
                                if key == "path" {
                                    continue;
                                }
                                let val_str = format_toml_value(val);
                                parts.push(format!("{} = {}", key, val_str));
                            }

                            return Some(format!("{} = {{ {} }}", name, parts.join(", ")));
                        } else {
                            eprintln!(
                                "[FerrumPy] Warning: Failed to resolve path dependency '{}' with workspace deps",
                                name
                            );
                            return None;
                        }
                    } else {
                        // No workspace deps - just use absolute path
                        let mut parts = Vec::new();
                        parts.push(format!("path = \"{}\"", absolute_path.display()));

                        for (key, val) in t {
                            if key == "path" {
                                continue;
                            }
                            let val_str = format_toml_value(val);
                            parts.push(format!("{} = {}", key, val_str));
                        }

                        return Some(format!("{} = {{ {} }}", name, parts.join(", ")));
                    }
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

/// Create a resolved copy of a path dependency with workspace deps replaced
/// Returns the path to the resolved copy, or None if failed
fn create_resolved_path_dep(
    name: &str,
    source_path: &Path,
    workspace_deps: &Option<toml::value::Table>,
    path_base: &Path,
    output_dir: &Path,
) -> Option<PathBuf> {
    // Create deps directory in output
    let deps_dir = output_dir.join("deps");
    let dest_dir = deps_dir.join(name);

    if let Err(e) = fs::create_dir_all(&dest_dir) {
        eprintln!("[FerrumPy] Failed to create deps dir: {}", e);
        return None;
    }

    // Copy and transform src directory (add serde derives to types)
    let src_dir = source_path.join("src");
    if src_dir.exists() {
        if let Err(e) = copy_and_transform_src(&src_dir, &dest_dir.join("src"), true) {
            eprintln!("[FerrumPy] Failed to copy and transform src: {}", e);
            return None;
        }
    }

    // Read and resolve the Cargo.toml
    let cargo_path = source_path.join("Cargo.toml");
    let content = match fs::read_to_string(&cargo_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[FerrumPy] Failed to read Cargo.toml: {}", e);
            return None;
        }
    };

    let toml_val: toml::Value = match content.parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[FerrumPy] Failed to parse Cargo.toml: {}", e);
            return None;
        }
    };

    // Generate resolved Cargo.toml
    let resolved_cargo =
        generate_resolved_cargo_toml(&toml_val, workspace_deps, path_base, output_dir);

    if let Err(e) = fs::write(dest_dir.join("Cargo.toml"), &resolved_cargo) {
        eprintln!("[FerrumPy] Failed to write resolved Cargo.toml: {}", e);
        return None;
    }

    if std::env::var("FERRUMPY_DEBUG").is_ok() {
        eprintln!(
            "[libgen] Created resolved copy of '{}' at {:?}",
            name, dest_dir
        );
    }

    Some(dest_dir)
}

/// Generate a resolved Cargo.toml with workspace deps replaced
fn generate_resolved_cargo_toml(
    toml_val: &toml::Value,
    workspace_deps: &Option<toml::value::Table>,
    path_base: &Path,
    output_dir: &Path,
) -> String {
    let mut result = String::new();

    // Copy [package] section, removing workspace inheritance
    if let Some(package) = toml_val.get("package") {
        result.push_str("[package]\n");
        if let Some(table) = package.as_table() {
            for (key, val) in table {
                // Skip workspace inherited fields
                if let toml::Value::Table(inner) = val {
                    if inner.get("workspace").is_some() {
                        continue;
                    }
                }
                // Simple values
                match val {
                    toml::Value::String(s) => result.push_str(&format!("{} = \"{}\"\n", key, s)),
                    toml::Value::Integer(i) => result.push_str(&format!("{} = {}\n", key, i)),
                    toml::Value::Boolean(b) => result.push_str(&format!("{} = {}\n", key, b)),
                    _ => {}
                }
            }
        }
        // Add default edition if not present
        if !result.contains("edition") {
            result.push_str("edition = \"2021\"\n");
        }
        result.push('\n');
    }

    // Copy [lib] section if present
    if let Some(lib) = toml_val.get("lib") {
        result.push_str("[lib]\n");
        if let Some(table) = lib.as_table() {
            for (key, val) in table {
                result.push_str(&format!("{} = {}\n", key, format_toml_value(val)));
            }
        }
        result.push('\n');
    }

    // Resolve [dependencies] - always add serde for derive macros
    result.push_str("[dependencies]\n");
    result.push_str("serde = { version = \"1\", features = [\"derive\"] }\n");

    if let Some(deps) = toml_val.get("dependencies") {
        if let Some(table) = deps.as_table() {
            for (dep_name, dep_val) in table {
                // Skip serde if already in deps
                if dep_name == "serde" || dep_name == "serde_json" {
                    continue;
                }
                if let Some(resolved) =
                    resolve_dependency(dep_name, dep_val, workspace_deps, path_base, output_dir)
                {
                    result.push_str(&resolved);
                    result.push('\n');
                }
            }
        }
    }
    result.push('\n');

    result
}

/// Copy src directory and transform Rust files (add serde derives)
fn copy_and_transform_src(src: &Path, dst: &Path, add_serde: bool) -> anyhow::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_and_transform_src(&src_path, &dst_path, add_serde)?;
        } else {
            // Check if it's a Rust file
            if src_path.extension().and_then(|e| e.to_str()) == Some("rs") {
                // Read and transform the file
                let content = fs::read_to_string(&src_path)?;
                match transformer::transform_module(&content, add_serde) {
                    Ok(transformed) => {
                        fs::write(&dst_path, transformed)?;
                    }
                    Err(e) => {
                        // If transformation fails, just copy the original
                        eprintln!(
                            "[FerrumPy] Warning: Could not transform {:?}: {}",
                            src_path, e
                        );
                        fs::copy(&src_path, &dst_path)?;
                    }
                }
            } else {
                // Non-Rust files, just copy
                fs::copy(&src_path, &dst_path)?;
            }
        }
    }
    Ok(())
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
        let dummy_output = Path::new("/tmp/output");
        let result = resolve_dependency("serde", &val, &None, dummy_path, dummy_output);
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
        let dummy_output = Path::new("/tmp/output");
        let result = resolve_dependency("serde", &val, &None, dummy_path, dummy_output).unwrap();
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
        let dummy_output = Path::new("/tmp/output");
        let result = resolve_dependency(
            "bitflags",
            &dep_val,
            &Some(ws_deps),
            dummy_path,
            dummy_output,
        );
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
        let dummy_output = Path::new("/tmp/output");
        let result =
            resolve_dependency("tokio", &dep_val, &Some(ws_deps), dummy_path, dummy_output)
                .unwrap();
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
        let dummy_output = Path::new("/tmp/output");
        let result = resolve_dependency("unknown_dep", &dep_val, &None, dummy_path, dummy_output);
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
        let dummy_output = Path::new("/tmp/output");
        let result = resolve_dependency(
            "missing_dep",
            &dep_val,
            &Some(ws_deps),
            dummy_path,
            dummy_output,
        );
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
        let dummy_output = Path::new("/tmp/output");
        let result =
            resolve_dependency("other_crate", &dep_val, &None, project_path, dummy_output).unwrap();

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
        let dummy_output = Path::new("/tmp/output");
        let result =
            resolve_dependency("my_lib", &dep_val, &None, project_path, dummy_output).unwrap();

        assert!(
            result.contains("path = \"/workspace/project/crates/my_lib\""),
            "Got: {}",
            result
        );
        assert!(result.contains("features = [\"async\"]"), "Got: {}", result);
    }

    #[test]
    fn test_resolve_dependency_workspace_path_uses_workspace_base() {
        // When workspace dep has path, it should use workspace root as base, not project path
        // Simulate { workspace = true }
        let mut dep_table = toml::value::Table::new();
        dep_table.insert("workspace".to_string(), toml::Value::Boolean(true));
        let dep_val = toml::Value::Table(dep_table);

        // Simulate workspace.dependencies.common = { path = "crates/common" }
        let mut common_table = toml::value::Table::new();
        common_table.insert(
            "path".to_string(),
            toml::Value::String("crates/common".to_string()),
        );
        let mut ws_deps = toml::value::Table::new();
        ws_deps.insert("common".to_string(), toml::Value::Table(common_table));

        // path_base should be workspace root, not project path
        // In real usage, this is the workspace root from find_workspace_dependencies
        let workspace_root = Path::new("/workspace/myproject");
        let dummy_output = Path::new("/tmp/output");
        let result = resolve_dependency(
            "common",
            &dep_val,
            &Some(ws_deps),
            workspace_root,
            dummy_output,
        )
        .unwrap();

        // Path should be relative to workspace root, not some subdir
        assert!(
            result.contains("path = \"/workspace/myproject/crates/common\""),
            "Expected path relative to workspace root. Got: {}",
            result
        );
    }
}
