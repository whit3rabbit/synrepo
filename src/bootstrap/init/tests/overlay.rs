use super::{bootstrap, isolated_home};
use crate::config::Config;
use crate::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;

#[test]
fn bootstrap_fresh_init_materializes_empty_overlay_store() {
    let _home = isolated_home();
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "overlay token\n").unwrap();

    bootstrap(repo.path(), None, false).unwrap();

    let overlay_dir = Config::synrepo_dir(repo.path()).join("overlay");
    let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
    let overlay = SqliteOverlayStore::open_existing(&overlay_dir).unwrap();

    assert!(overlay_db.exists(), "init must materialize overlay.db");
    assert_eq!(
        overlay.stored_row_count().unwrap(),
        0,
        "default init creates tables but no overlay data"
    );
}
