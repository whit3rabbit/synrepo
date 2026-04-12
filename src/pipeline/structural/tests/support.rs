use crate::store::sqlite::SqliteGraphStore;

pub(super) fn open_graph(repo: &tempfile::TempDir) -> SqliteGraphStore {
    let graph_dir = repo.path().join(".synrepo/graph");
    SqliteGraphStore::open(&graph_dir).unwrap()
}
