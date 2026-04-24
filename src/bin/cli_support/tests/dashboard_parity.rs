//! Parity test between the CLI `status` renderer and the dashboard view
//! models. Both surfaces consume the same `StatusSnapshot` and must agree on
//! the key operator-facing facts: mode, reconcile health, export freshness,
//! overlay cost summary.
//!
//! Task 6.3 of `openspec/changes/runtime-dashboard-v1/tasks.md`.

use synrepo::bootstrap::runtime_probe::AgentIntegration;
use synrepo::surface::status_snapshot::{build_status_snapshot, StatusOptions};
use synrepo::tui::probe::{build_header_vm, build_health_vm};
use tempfile::tempdir;

use super::super::commands::status_output;
use super::support::seed_graph;

#[test]
fn cli_text_and_dashboard_vm_agree_on_key_fields() {
    let repo = tempdir().unwrap();
    // seed_graph bootstraps and populates a file/symbol/concept triple so the
    // snapshot has real graph stats and a readable config.
    seed_graph(repo.path());

    let snapshot = build_status_snapshot(
        repo.path(),
        StatusOptions {
            recent: false,
            full: false,
        },
    );

    let cli_text = status_output(repo.path(), false, false, false).unwrap();

    let header = build_header_vm(
        "test-repo".to_string(),
        &snapshot,
        &AgentIntegration::Absent,
        None,
    );
    let health = build_health_vm(&snapshot);

    // Mode: CLI prints `mode:  <mode>`, header VM carries `mode_label`.
    let mode = snapshot
        .config
        .as_ref()
        .expect("initialized implies config loaded")
        .mode
        .to_string();
    assert_eq!(header.mode_label, mode);
    assert!(
        cli_text.contains(&format!("mode:         {mode}")),
        "CLI output missing mode line: {cli_text}"
    );

    // Export freshness: CLI prints the raw string, health VM surfaces it as
    // the `export` row value.
    let export_row = health
        .rows
        .iter()
        .find(|r| r.label == "export")
        .expect("health VM must surface an export row when initialized");
    assert_eq!(export_row.value, snapshot.export_freshness);
    assert!(
        cli_text.contains(&snapshot.export_freshness),
        "CLI output missing export freshness line: {cli_text}"
    );

    // Overlay cost: same string on both surfaces.
    let overlay_row = health
        .rows
        .iter()
        .find(|r| r.label == "overlay cost")
        .expect("health VM must surface an overlay cost row");
    assert_eq!(overlay_row.value, snapshot.overlay_cost_summary);
    assert!(
        cli_text.contains(&snapshot.overlay_cost_summary),
        "CLI output missing overlay cost line: {cli_text}"
    );

    // Reconcile health: both surfaces derive from the same diagnostics enum.
    // Bare-seeded repo has not run reconcile, so it should be Unknown on both.
    assert_eq!(header.reconcile_label, "unknown");
    assert!(
        cli_text.contains("reconcile:    unknown"),
        "CLI output missing reconcile unknown line: {cli_text}"
    );
}

#[test]
fn uninitialized_parity_not_initialized_on_both_surfaces() {
    // Config::load falls back to ~/.synrepo/config.toml; redirect HOME to an
    // empty tempdir under the shared lock so the developer's real user-scoped
    // config can't satisfy the load and make the tempdir look initialized.
    let _lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _home_guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());

    let repo = tempdir().unwrap();
    // No bootstrap: snapshot will report `initialized=false`.
    let snapshot = build_status_snapshot(
        repo.path(),
        StatusOptions {
            recent: false,
            full: false,
        },
    );
    assert!(!snapshot.initialized);

    let cli_text = status_output(repo.path(), false, false, false).unwrap();
    let header = build_header_vm(
        "test-repo".to_string(),
        &snapshot,
        &AgentIntegration::Absent,
        None,
    );
    let health = build_health_vm(&snapshot);

    assert!(
        cli_text.contains("synrepo status: not initialized"),
        "CLI output should report not initialized: {cli_text}"
    );
    assert_eq!(header.mode_label, "uninitialized");
    // Health VM collapses to a single not-initialized row.
    assert_eq!(health.rows.len(), 1);
    assert_eq!(health.rows[0].value, "not initialized");
}
