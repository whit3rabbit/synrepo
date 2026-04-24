use tempfile::tempdir;

use super::support::init_empty_graph;
use crate::config::Config;
use crate::pipeline::export::{write_exports, ExportFormat};
use crate::surface::card::Budget;

#[test]
fn export_rejects_traversing_export_dir() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "../escape".to_string(),
        ..Config::default()
    };

    let err = match write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Markdown,
        Budget::Normal,
        true,
    ) {
        Ok(_) => panic!("expected write_exports to reject out-of-repo export_dir"),
        Err(err) => err,
    };

    let msg = err.to_string();
    assert!(
        msg.contains("export_dir") && msg.contains("relative path"),
        "expected export_dir rejection message, got: {msg}"
    );

    // The out-of-repo path must not have been created.
    let escape_path = repo.path().parent().unwrap().join("escape");
    assert!(
        !escape_path.exists(),
        "traversing export_dir must not create {escape_path:?}"
    );
}

#[test]
fn export_rejects_absolute_export_dir() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "/tmp/synrepo-absolute-escape".to_string(),
        ..Config::default()
    };

    let err = match write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Markdown,
        Budget::Normal,
        true,
    ) {
        Ok(_) => panic!("expected write_exports to reject out-of-repo export_dir"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("export_dir"));
}
