use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Parse a JSON file if it exists; fail loud with the file path if the content
/// is present but malformed, rather than silently discarding user config.
pub(crate) fn load_json_config(path: &Path) -> anyhow::Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(&content).map_err(|err| {
        anyhow!(
            "refusing to overwrite {}: file exists but is not valid JSON ({err}). \
             Fix or remove the file and re-run `synrepo setup`.",
            path.display()
        )
    })
}

/// Write JSON back to disk with pretty-printing and a trailing newline.
pub(crate) fn write_json_config(path: &Path, value: &Value) -> anyhow::Result<()> {
    let mut out = serde_json::to_string_pretty(value)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    out.push('\n');
    write_atomic(path, out.as_bytes())
}

pub(super) fn write_atomic(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    synrepo::util::atomic_write(path, contents)
        .with_context(|| format!("failed to atomically write {}", path.display()))
}
