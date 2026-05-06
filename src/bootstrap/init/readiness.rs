//! Bootstrap readiness helpers.

use std::path::Path;

use crate::bootstrap::report::DegradedCapability;
use crate::bootstrap::runtime_probe::probe;
use crate::config::Config;
use crate::surface::readiness::ReadinessMatrix;
use crate::surface::status_snapshot::{build_status_snapshot, StatusOptions};

pub(super) fn collect_degraded_capabilities(
    repo_root: &Path,
    _synrepo_dir: &Path,
    config: &Config,
) -> Vec<DegradedCapability> {
    let snapshot = build_status_snapshot(
        repo_root,
        StatusOptions {
            recent: false,
            full: false,
        },
    );
    if !snapshot.initialized {
        return Vec::new();
    }
    let probe_report = probe(repo_root);
    let matrix = ReadinessMatrix::build(repo_root, &probe_report, &snapshot, config);
    matrix
        .degraded_rows()
        .map(|row| DegradedCapability {
            capability: row.capability.as_str().to_string(),
            state: row.state.as_str().to_string(),
            detail: row.detail.clone(),
            next_action: row.next_action.clone(),
        })
        .collect()
}
