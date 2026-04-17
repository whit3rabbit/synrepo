//! Export freshness for status.

use std::path::Path;

use synrepo::{
    config::Config,
    pipeline::{export::load_manifest, watch::load_reconcile_state},
};

/// Describe export freshness for status output.
pub fn export_freshness_summary(repo_root: &Path, synrepo_dir: &Path, config: &Config) -> String {
    let manifest = load_manifest(repo_root, config);
    match manifest {
        None => "absent (run `synrepo export` to generate)".to_string(),
        Some(m) => {
            let current_epoch = load_reconcile_state(synrepo_dir)
                .map(|r| r.last_reconcile_at)
                .unwrap_or_default();
            if m.last_reconcile_at == current_epoch {
                format!("current ({}, {})", m.format.as_str(), m.budget)
            } else {
                format!(
                    "stale (generated at {}, current epoch {})",
                    m.last_reconcile_at, current_epoch
                )
            }
        }
    }
}
