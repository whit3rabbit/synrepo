use super::{seed_graph, status_output};
use synrepo::config::Config;
use synrepo::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;

#[test]
fn status_json_reports_missing_overlay_store() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let overlay_db = SqliteOverlayStore::db_path(&Config::synrepo_dir(repo.path()).join("overlay"));
    std::fs::remove_file(&overlay_db).unwrap();

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["overlay_state"], "missing", "{json}");
}
