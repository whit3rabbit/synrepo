//! Shared test helpers for compiler sub-module tests.

use crate::{
    config::Config, pipeline::structural::run_structural_compile, store::sqlite::SqliteGraphStore,
};

pub fn bootstrap(repo: &tempfile::TempDir) -> SqliteGraphStore {
    let graph_dir = repo.path().join(".synrepo/graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    run_structural_compile(repo.path(), &Config::default(), &mut graph).unwrap();
    graph
}
