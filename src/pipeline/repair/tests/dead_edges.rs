use tempfile::tempdir;

use super::support::setup_repo_for_sync;
use crate::{
    config::Config,
    core::{
        ids::{ConceptNodeId, EdgeId, NodeId, SymbolNodeId},
        provenance::Provenance,
    },
    pipeline::repair::{execute_sync, RepairSurface, SyncOptions},
    store::sqlite::SqliteGraphStore,
    structure::graph::{Edge, EdgeKind, Epistemic, GraphStore},
};

#[test]
fn sync_prunes_dead_edges_and_commits_batch() {
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);
    let edge = Edge {
        id: EdgeId(1),
        from: NodeId::Concept(ConceptNodeId(1)),
        to: NodeId::Symbol(SymbolNodeId(2)),
        kind: EdgeKind::References,
        owner_file_id: None,
        last_observed_rev: None,
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: Provenance::structural("test", "rev001", Vec::new()),
    };

    {
        let mut graph = SqliteGraphStore::open(&synrepo_dir.join("graph")).unwrap();
        graph.insert_edge(edge.clone()).unwrap();
        graph
            .write_drift_scores(&[(edge.id, 1.0)], "rev001")
            .unwrap();
    }

    let summary = execute_sync(
        &repo,
        &synrepo_dir,
        &Config::default(),
        SyncOptions::default(),
    )
    .unwrap();

    assert!(
        summary
            .repaired
            .iter()
            .any(|finding| finding.surface == RepairSurface::EdgeDrift),
        "edge drift should be reported as repaired: {:?}",
        summary.repaired
    );
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    assert!(
        graph
            .outbound(NodeId::Concept(ConceptNodeId(1)), None)
            .unwrap()
            .is_empty(),
        "dead edge should be deleted after sync commits"
    );
}
