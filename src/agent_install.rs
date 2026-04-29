//! Shared identity and helpers for `agent-config` synrepo installs.
//!
//! Both the bin-side install/remove flow and the lib-side bootstrap probe and
//! repair surfaces need to talk to `agent-config` using the same owner/name
//! pair. Centralized here so the two crates cannot drift.

#![allow(missing_docs)]

use std::path::PathBuf;

pub const SYNREPO_INSTALL_NAME: &str = "synrepo";
pub const SYNREPO_INSTALL_OWNER: &str = "synrepo";

/// Resolve the on-disk `SKILL.md` path from an `agent-config` skill status
/// report. Used by both the bin-side install path probe and the lib-side
/// bootstrap detection probe.
pub fn skill_manifest_path(report: agent_config::StatusReport) -> Option<PathBuf> {
    report
        .files
        .iter()
        .find_map(|status| match status {
            agent_config::PathStatus::Exists { path }
            | agent_config::PathStatus::Missing { path }
            | agent_config::PathStatus::Invalid { path, .. }
                if path.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") =>
            {
                Some(path.clone())
            }
            _ => None,
        })
        .or_else(|| report.config_path.map(|dir| dir.join("SKILL.md")))
}
