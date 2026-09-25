//! Low-level file I/O for the registry. Kept separate from the public API in
//! `mod.rs` so tests can drive load/save against a temp path without going
//! through `$HOME`-based `registry_path()`.

use std::path::Path;

use anyhow::Context;

use super::{Registry, SCHEMA_VERSION};

pub(super) fn registry_path() -> Option<std::path::PathBuf> {
    let home = crate::config::home_dir()?;
    // Test harnesses that did not redirect HOME must not add their temporary
    // repositories to the developer's registry. Redirected temp HOME wins.
    if is_test_harness()
        && !std::fs::canonicalize(&home)
            .ok()
            .zip(std::fs::canonicalize(std::env::temp_dir()).ok())
            .is_some_and(|(home, temp)| home.starts_with(temp))
    {
        static TEST_REGISTRY: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        return Some(
            TEST_REGISTRY
                .get_or_init(|| {
                    std::env::temp_dir()
                        .join(format!(
                            "synrepo-test-registry-{}-{}",
                            std::process::id(),
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos()
                        ))
                        .join("projects.toml")
                })
                .clone(),
        );
    }
    Some(home.join(".synrepo").join("projects.toml"))
}

fn is_test_harness() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let in_deps = exe
        .parent()
        .and_then(|dir| dir.file_name())
        .is_some_and(|name| name == "deps");
    let hashed_name = exe
        .file_stem()
        .and_then(|name| name.to_str())
        .and_then(|name| name.rsplit_once('-'))
        .is_some_and(|(_, hash)| hash.len() >= 12 && hash.bytes().all(|ch| ch.is_ascii_hexdigit()));
    in_deps && hashed_name
}

/// Read the registry file at `path`. Returns an empty [`Registry`] if the file
/// does not exist or is empty. Parses strictly otherwise, so a malformed file
/// surfaces a clear error instead of being silently discarded.
pub fn load_from(path: &Path) -> anyhow::Result<Registry> {
    if !path.exists() {
        return Ok(Registry::default());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read registry {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(Registry::default());
    }
    let registry: Registry = toml::from_str(&text)
        .with_context(|| format!("failed to parse registry {}", path.display()))?;
    Ok(registry)
}

/// Write the registry to `path` atomically. Creates the parent directory if it
/// doesn't exist. Always stamps the current [`SCHEMA_VERSION`] so older files
/// are upgraded on first write.
pub fn save_to(path: &Path, registry: &Registry) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create registry parent directory {}",
                parent.display()
            )
        })?;
    }
    let mut stamped = registry.clone();
    stamped.schema_version = SCHEMA_VERSION;
    let text = toml::to_string_pretty(&stamped)
        .with_context(|| format!("failed to serialize registry {}", path.display()))?;
    crate::util::atomic_write(path, text.as_bytes())
        .with_context(|| format!("failed to write registry {}", path.display()))?;
    Ok(())
}
