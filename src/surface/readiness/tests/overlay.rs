use super::*;

#[test]
fn overlay_row_reports_ready_empty_as_supported() {
    let snapshot = base_snapshot(base_diagnostics());
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::Overlay);
    assert_eq!(row.state, ReadinessState::Supported);
    assert_eq!(row.detail, "ready_empty; no overlay entries yet");
    assert_eq!(row.next_action, None);
}

#[test]
fn overlay_row_reports_missing_store_as_unavailable() {
    let mut snapshot = base_snapshot(base_diagnostics());
    snapshot.overlay_state = OverlayState::Missing;
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::Overlay);
    assert_eq!(row.state, ReadinessState::Unavailable);
    assert_eq!(row.next_action.as_deref(), Some("run `synrepo init`"));
}

#[test]
fn overlay_row_reports_corrupt_store_as_blocked() {
    let mut snapshot = base_snapshot(base_diagnostics());
    snapshot.overlay_state = OverlayState::Error;
    snapshot.commentary_coverage = CommentaryCoverage {
        total: None,
        fresh: None,
        estimated_fresh: None,
        estimated_stale_ratio: None,
        estimate_confidence: None,
        display: "unavailable (open failed)".to_string(),
    };
    let matrix =
        ReadinessMatrix::build(&repo_root(), &ready_probe(), &snapshot, &Config::default());
    let row = find_row(&matrix, Capability::Overlay);
    assert_eq!(row.state, ReadinessState::Blocked);
}
