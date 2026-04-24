//! Status command implementation.
//!
//! Pure formatter over `synrepo::surface::status_snapshot::StatusSnapshot`.

mod helpers;
mod json;
mod text;

pub(crate) use helpers::render_watch_summary;

use std::path::Path;

use synrepo::bootstrap::runtime_probe::probe;
use synrepo::surface::readiness::ReadinessMatrix;
use synrepo::surface::status_snapshot::StatusOptions;

/// Print operational health: mode, graph counts, reconcile status, and watch state.
pub(crate) fn status(repo_root: &Path, json: bool, recent: bool, full: bool) -> anyhow::Result<()> {
    let rendered = status_output(repo_root, json, recent, full)?;
    print!("{rendered}");
    Ok(())
}

/// Render the status output as a String. Used by `cli.rs` for the non-TTY
/// fallback under bare `synrepo` on a ready repo, and by tests.
pub(crate) fn status_output(
    repo_root: &Path,
    json: bool,
    recent: bool,
    full: bool,
) -> anyhow::Result<String> {
    let snapshot = synrepo::surface::status_snapshot::build_status_snapshot(
        repo_root,
        StatusOptions { recent, full },
    );
    // Build the capability readiness matrix so status, doctor, and dashboard
    // all report degradation using the same labels.
    let matrix = snapshot.initialized.then(|| {
        let probe_report = probe(repo_root);
        let cfg = snapshot.config.clone().unwrap_or_default();
        ReadinessMatrix::build(repo_root, &probe_report, &snapshot, &cfg)
    });
    let mut out = String::new();
    if json {
        json::write_status_json(&mut out, &snapshot, matrix.as_ref())?;
    } else {
        text::write_status_text(&mut out, &snapshot, matrix.as_ref(), full);
    }
    Ok(out)
}

/// Shared between text and JSON formatters for explain pricing basis detection.
fn openrouter_live_pricing_used(
    totals: &synrepo::pipeline::explain::accounting::ExplainTotals,
) -> bool {
    totals
        .per_provider
        .get("openrouter")
        .and_then(|provider| provider.usd_cost)
        .is_some()
}
