//! Helpers that turn an `AgentTool` install into a registry entry.
//!
//! Kept here so both the bare `synrepo agent-setup <tool>` path (in
//! `commands/basic.rs`) and the full `synrepo setup <tool>` path (in
//! `commands/setup.rs`) share one code path without creating a module cycle
//! between those two files.

use std::path::Path;

use synrepo::pipeline::writer::now_rfc3339;
use synrepo::registry::{self, AgentEntry};

use super::AgentTool;

/// Best-effort registry update after a successful shim or full install.
///
/// A registry write failure is NEVER surfaced to the user: the removal path
/// has a filesystem-scan fallback, so a missing `~/.synrepo/projects.toml`
/// entry is at worst a very small perf hit at remove time. Errors are logged
/// via `tracing::warn!` and swallowed.
///
/// `wrote_mcp_config` controls whether the entry records a `mcp_config_path`.
/// Pass `true` only from code paths that actually wrote an MCP server entry
/// (`synrepo setup` on an automated-tier tool). Pass `false` from the bare
/// `synrepo agent-setup <tool>` path, which only writes a shim file.
///
/// `mcp_backup_path` is the repo-relative path of the pristine-state `.bak`
/// sidecar when one exists (freshly created or pre-existing), `None` otherwise.
/// Produced by `step_backup_mcp_config` before the MCP write.
pub(crate) fn record_install_best_effort(
    repo_root: &Path,
    tool: AgentTool,
    wrote_mcp_config: bool,
    mcp_backup_path: Option<String>,
) {
    let shim_path = match tool
        .output_path(repo_root)
        .strip_prefix(repo_root)
        .map(Path::to_path_buf)
    {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => tool.output_path(repo_root).to_string_lossy().into_owned(),
    };
    let mcp_config_path = if wrote_mcp_config {
        tool.mcp_config_relative_path().map(|s| s.to_string())
    } else {
        None
    };
    let entry = AgentEntry {
        tool: tool.canonical_name().to_string(),
        shim_path,
        mcp_config_path,
        mcp_backup_path,
        installed_at: now_rfc3339(),
    };
    if let Err(err) = registry::record_agent(repo_root, entry) {
        tracing::warn!(
            error = %err,
            tool = tool.canonical_name(),
            "install registry update skipped after {} install",
            tool.display_name()
        );
    }
}
