use std::fs;

use super::*;

#[test]
fn project_layout_row_reports_detected_excluded_roots() {
    let repo = tempfile::tempdir().unwrap();
    fs::write(repo.path().join("pubspec.yaml"), "name: app\n").unwrap();
    fs::create_dir_all(repo.path().join("lib")).unwrap();
    fs::create_dir_all(repo.path().join("test")).unwrap();
    let config = Config {
        roots: vec!["lib".to_string()],
        ..Config::default()
    };
    let snapshot = base_snapshot(base_diagnostics());

    let matrix = ReadinessMatrix::build(repo.path(), &ready_probe(), &snapshot, &config);
    let row = find_row(&matrix, Capability::ProjectLayout);

    assert_eq!(row.state, ReadinessState::Degraded);
    assert!(row.detail.contains("detected dart"));
    assert!(row.detail.contains("test"));
}
