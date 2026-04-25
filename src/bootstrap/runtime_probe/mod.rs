//! Read-only runtime probe.
//!
//! Classifies the on-disk `.synrepo/` state as `Uninitialized`, `Partial` (with a
//! structured list of missing components), or `Ready`, without acquiring the
//! writer lock or mutating any store. Consumed by the bare-`synrepo` entrypoint
//! router in `src/bin/cli.rs` and by the future dashboard TUI.
//!
//! Spec: `openspec/changes/runtime-dashboard-v1/specs/runtime-probe/spec.md`.

pub mod detection;
pub mod types;

pub use detection::{
    all_agent_targets, detect_agent_integration, detect_agent_targets, dirs_home, shim_output_path,
};
pub use types::{
    AgentIntegration, AgentTargetKind, Missing, ProbeReport, RoutingDecision, RuntimeClassification,
};

use std::{fs, path::Path};

use crate::config::Config;
use crate::store::compatibility;
use crate::store::sqlite::SqliteGraphStore;

/// Run the runtime probe against `repo_root`.
///
/// Read-only: no writer-lock acquisition, no store mutation, no log append.
/// Safe to call concurrently with an active watch service or writer.
pub fn probe(repo_root: &Path) -> ProbeReport {
    probe_with_home(repo_root, dirs_home().as_deref())
}

/// Lower-level probe entry point that accepts an explicit `home` override. The
/// public [`probe`] wrapper resolves `home` via the platform environment. Tests
/// use this form to avoid picking up the real user's `$HOME/.claude` etc.
pub fn probe_with_home(repo_root: &Path, home: Option<&Path>) -> ProbeReport {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let detected_agent_targets = detect_agent_targets(repo_root, home);

    let (classification, config_for_agent) = if synrepo_dir.exists() {
        classify_partial_or_ready(&synrepo_dir)
    } else {
        (RuntimeClassification::Uninitialized, None)
    };

    // Agent integration is computed unconditionally so the dashboard and
    // wizard can surface "you already wrote a shim" hints even on an
    // uninitialized repo.
    let agent_integration = detect_agent_integration(
        repo_root,
        &synrepo_dir,
        config_for_agent.as_ref(),
        &detected_agent_targets,
    );

    ProbeReport {
        classification,
        agent_integration,
        detected_agent_targets,
        synrepo_dir,
    }
}

fn classify_partial_or_ready(synrepo_dir: &Path) -> (RuntimeClassification, Option<Config>) {
    let mut missing: Vec<Missing> = Vec::new();

    // Check config.toml.
    let config_path = synrepo_dir.join("config.toml");
    let config: Option<Config> = if !config_path.exists() {
        missing.push(Missing::ConfigFile);
        None
    } else {
        match fs::read_to_string(&config_path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(config) => Some(config),
                Err(err) => {
                    missing.push(Missing::ConfigUnreadable {
                        detail: format!("invalid TOML: {err}"),
                    });
                    None
                }
            },
            Err(err) => {
                missing.push(Missing::ConfigUnreadable {
                    detail: err.to_string(),
                });
                None
            }
        }
    };

    // Check graph store readiness (`nodes.db` exists and can plausibly be
    // opened). This is intentionally tighter than
    // `compatibility::evaluate::store_is_materialized`, which only cares
    // whether the store path exists for migration-policy decisions.
    if !graph_store_materialized(synrepo_dir) {
        missing.push(Missing::GraphStore);
    }

    // Check compatibility evaluation. Use the loaded config or defaults so
    // an unreadable config.toml doesn't mask a compat issue from surfacing.
    let compat_config = config.clone().unwrap_or_default();
    match compatibility::evaluate_runtime(synrepo_dir, true, &compat_config) {
        Ok(report) => {
            if report.has_blocking_actions() {
                missing.push(Missing::CompatBlocked {
                    guidance: report.guidance_lines(),
                });
            }
        }
        Err(err) => {
            missing.push(Missing::CompatEvaluationFailed {
                detail: err.to_string(),
            });
        }
    }

    if missing.is_empty() {
        (RuntimeClassification::Ready, config)
    } else {
        (RuntimeClassification::Partial { missing }, config)
    }
}

fn graph_store_materialized(synrepo_dir: &Path) -> bool {
    let graph_dir = synrepo_dir.join("graph");
    // Fast path: the file does not exist (the common "uninitialized" case).
    // Avoids invoking the SQLite validator on every bare-`synrepo` startup
    // when the repo has never been initialized.
    if !graph_dir.join("nodes.db").exists() {
        return false;
    }
    // Tighter check: file exists AND is an openable SQLite database. A junk
    // file at the path (truncated, zero-byte, wrong content) classifies as
    // "missing" rather than routing the user into the dashboard only to fail
    // on the first graph read.
    SqliteGraphStore::validate_existing(&graph_dir).is_ok()
}

#[cfg(test)]
mod tests;
