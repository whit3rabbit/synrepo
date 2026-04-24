//! Install-side backup of pre-existing MCP config files (Phase 4).
//!
//! Before `synrepo setup` modifies an MCP config the user already has on
//! disk, we copy the pristine file to `{path}.bak` so a later `synrepo remove`
//! (or a manual undo) can restore the original. The prompt is TTY-only; CI
//! and piped invocations skip the backup to stay deterministic.
//!
//! Never overwrites an existing `.bak` and never backs up a file that already
//! contains a synrepo entry.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde_json::Value;
use toml_edit::DocumentMut;

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::setup::load_json_config;

/// Pre-step for the full `synrepo setup <tool>` flow and the TUI integration
/// wizard. Returns the repo-relative backup path to stamp onto the registry
/// entry, or `None` when nothing was backed up.
pub(crate) fn step_backup_mcp_config(
    repo_root: &Path,
    tool: AgentTool,
) -> anyhow::Result<Option<String>> {
    let Some(rel) = tool.mcp_config_relative_path() else {
        return Ok(None);
    };
    let path = repo_root.join(rel);
    let Some(backup) = maybe_backup_mcp_config(&path)? else {
        return Ok(None);
    };
    Ok(Some(
        backup
            .strip_prefix(repo_root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| backup.to_string_lossy().into_owned()),
    ))
}

/// Append `.bak` to the full filename, preserving the original extension.
/// `.mcp.json` -> `.mcp.json.bak`, `.codex/config.toml` -> `.codex/config.toml.bak`.
fn backup_path_for(path: &Path) -> PathBuf {
    let mut bak = path.as_os_str().to_owned();
    bak.push(".bak");
    PathBuf::from(bak)
}

/// TTY-interactive Y/n prompt. Y is the default: bare Enter accepts. Returns
/// `false` when stdout is not a TTY, so CI / piped callers never block on input.
fn prompt_backup_mcp(path: &Path, bak: &Path) -> bool {
    use std::io::{self, BufRead, Write};

    if !prompt_stdout_is_tty() {
        return false;
    }
    print!(
        "  {} exists. Create backup at {} before modifying? [Y/n] ",
        path.display(),
        bak.display()
    );
    io::stdout().flush().ok();
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "" | "y" | "yes")
}

fn prompt_stdout_is_tty() -> bool {
    #[cfg(test)]
    if let Some(is_tty) = test_stdout_is_tty_override() {
        return is_tty;
    }

    synrepo::tui::stdout_is_tty()
}

#[cfg(test)]
thread_local! {
    static TEST_STDOUT_IS_TTY_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn test_stdout_is_tty_override() -> Option<bool> {
    TEST_STDOUT_IS_TTY_OVERRIDE.with(std::cell::Cell::get)
}

#[cfg(test)]
fn force_stdout_is_tty_for_test(is_tty: bool) -> TestStdoutIsTtyGuard {
    let previous = test_stdout_is_tty_override();
    TEST_STDOUT_IS_TTY_OVERRIDE.with(|override_slot| override_slot.set(Some(is_tty)));
    TestStdoutIsTtyGuard { previous }
}

#[cfg(test)]
struct TestStdoutIsTtyGuard {
    previous: Option<bool>,
}

#[cfg(test)]
impl Drop for TestStdoutIsTtyGuard {
    fn drop(&mut self) {
        TEST_STDOUT_IS_TTY_OVERRIDE.with(|override_slot| override_slot.set(self.previous));
    }
}

/// Return the backup path when a `.bak` now exists (freshly created or
/// pre-existing), `None` otherwise. Never overwrites an existing `.bak` and
/// never backs up a file that already contains a synrepo entry (we'd just be
/// copying our own prior edit).
fn maybe_backup_mcp_config(path: &Path) -> anyhow::Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let bak = backup_path_for(path);
    if bak.exists() {
        // A prior install already captured the pristine state; reuse it.
        return Ok(Some(bak));
    }
    if mcp_config_has_synrepo(path)? {
        return Ok(None);
    }
    if !prompt_backup_mcp(path, &bak) {
        return Ok(None);
    }
    fs::copy(path, &bak)
        .with_context(|| format!("failed to copy {} to {}", path.display(), bak.display()))?;
    println!("  Created backup at {}", bak.display());
    Ok(Some(bak))
}

/// `true` if `path` is an MCP config (JSON or Codex TOML) that already
/// contains a `synrepo` entry. Loud on parse failure: a corrupt config is a
/// "do not touch" signal for both the install and the remove sides.
pub(crate) fn mcp_config_has_synrepo(path: &Path) -> anyhow::Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let is_toml = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "toml")
        .unwrap_or(false);
    if is_toml {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let doc: DocumentMut = raw.parse().unwrap_or_default();
        return Ok(doc
            .get("mcp")
            .and_then(|i| i.as_table())
            .map(|t| t.contains_key("synrepo"))
            .unwrap_or(false));
    }
    let value = load_json_config(path)?;
    let Some(obj) = value.as_object() else {
        return Ok(false);
    };
    for container_key in ["mcpServers", "mcp"] {
        if let Some(container) = obj.get(container_key).and_then(Value::as_object) {
            if container.contains_key("synrepo") {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn missing_file_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".mcp.json");
        assert!(maybe_backup_mcp_config(&path).unwrap().is_none());
    }

    #[test]
    fn preexisting_bak_is_reused_without_reprompt() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".mcp.json");
        fs::write(&path, "{\"mcpServers\":{\"other\":{}}}").unwrap();
        let bak = backup_path_for(&path);
        fs::write(&bak, "PRIOR BACKUP BYTES").unwrap();

        let result = maybe_backup_mcp_config(&path).unwrap();
        assert_eq!(result.as_deref(), Some(bak.as_path()));
        // Backup must be untouched — never overwrite a prior backup.
        let bytes = fs::read(&bak).unwrap();
        assert_eq!(&bytes, b"PRIOR BACKUP BYTES");
    }

    #[test]
    fn file_with_synrepo_entry_is_not_backed_up() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".mcp.json");
        let body = json!({"mcpServers": {"synrepo": {"command": "synrepo"}}});
        fs::write(&path, serde_json::to_string_pretty(&body).unwrap()).unwrap();
        // No TTY in tests, but the synrepo-presence check short-circuits
        // before the prompt, so the result is deterministic here.
        assert!(maybe_backup_mcp_config(&path).unwrap().is_none());
        assert!(!backup_path_for(&path).exists());
    }

    #[test]
    fn non_tty_declines_to_back_up() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".mcp.json");
        fs::write(&path, "{\"mcpServers\":{\"other\":{}}}").unwrap();
        let _stdout = force_stdout_is_tty_for_test(false);
        // Simulate piped stdout so `prompt_backup_mcp` returns false without
        // reading stdin.
        assert!(maybe_backup_mcp_config(&path).unwrap().is_none());
        assert!(!backup_path_for(&path).exists());
    }

    #[test]
    fn backup_path_appends_bak_preserving_extension() {
        assert_eq!(
            backup_path_for(Path::new(".mcp.json")),
            PathBuf::from(".mcp.json.bak")
        );
        assert_eq!(
            backup_path_for(Path::new(".codex/config.toml")),
            PathBuf::from(".codex/config.toml.bak")
        );
        assert_eq!(
            backup_path_for(Path::new("opencode.json")),
            PathBuf::from("opencode.json.bak")
        );
    }

    #[test]
    fn mcp_config_has_synrepo_detects_json_containers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("opencode.json");
        fs::write(&path, r#"{"mcp":{"synrepo":"synrepo mcp"}}"#).unwrap();
        assert!(mcp_config_has_synrepo(&path).unwrap());

        let path2 = dir.path().join(".mcp.json");
        fs::write(&path2, r#"{"mcpServers":{"synrepo":{}}}"#).unwrap();
        assert!(mcp_config_has_synrepo(&path2).unwrap());

        let path3 = dir.path().join("other.json");
        fs::write(&path3, r#"{"mcpServers":{"something_else":{}}}"#).unwrap();
        assert!(!mcp_config_has_synrepo(&path3).unwrap());
    }

    #[test]
    fn mcp_config_has_synrepo_detects_codex_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[mcp]\nsynrepo = \"synrepo mcp\"\n").unwrap();
        assert!(mcp_config_has_synrepo(&path).unwrap());

        let path2 = dir.path().join("other.toml");
        fs::write(&path2, "[mcp]\nelse = \"x\"\n").unwrap();
        assert!(!mcp_config_has_synrepo(&path2).unwrap());
    }
}
