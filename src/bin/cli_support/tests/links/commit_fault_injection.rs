use synrepo::config::Config;
use synrepo::core::ids::{EdgeId, NodeId};
use synrepo::overlay::{OverlayEdgeKind, OverlayStore};
use synrepo::store::overlay::{format_candidate_id, SqliteOverlayStore};
use synrepo::store::sqlite::SqliteGraphStore;
use synrepo::structure::graph::{Edge, EdgeKind, GraphStore};

use super::commands::{CommitArgs, LinksCommitStore, RealLinksStore};
use super::{commands, sample_link, setup_curated_link_env};

// `{phase}_fails_once` fires exactly once so the subsequent retry with real
// stores exercises recovery.
#[derive(Default)]
struct FailureSwitches {
    insert_edge_fails_once: bool,
    mark_promoted_fails_once: bool,
    delete_edge_fails_once: bool,
}

struct FailingStore<'a> {
    inner: RealLinksStore<'a>,
    switches: FailureSwitches,
}

impl LinksCommitStore for FailingStore<'_> {
    fn mark_pending(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
    ) -> anyhow::Result<()> {
        self.inner.mark_pending(from, to, kind, reviewer)
    }

    fn insert_edge(&mut self, edge: Edge) -> anyhow::Result<()> {
        if self.switches.insert_edge_fails_once {
            self.switches.insert_edge_fails_once = false;
            anyhow::bail!("injected: insert_edge failure");
        }
        self.inner.insert_edge(edge)
    }

    fn mark_promoted(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
        edge_id: &str,
    ) -> anyhow::Result<()> {
        if self.switches.mark_promoted_fails_once {
            self.switches.mark_promoted_fails_once = false;
            anyhow::bail!("injected: mark_promoted failure");
        }
        self.inner.mark_promoted(from, to, kind, reviewer, edge_id)
    }

    fn delete_edge(&mut self, edge_id: EdgeId) -> anyhow::Result<()> {
        if self.switches.delete_edge_fails_once {
            self.switches.delete_edge_fails_once = false;
            anyhow::bail!("injected: delete_edge (compensation) failure");
        }
        self.inner.delete_edge(edge_id)
    }
}

fn curated_edge_id(from: NodeId, to: NodeId, kind: EdgeKind) -> EdgeId {
    synrepo::pipeline::structural::derive_edge_id(from, to, kind)
}

// Direct SQL read: the trait surface does not expose intermediate states like
// `pending_promotion` that the fault-injection tests need to observe.
fn read_state(
    overlay_dir: &std::path::Path,
    from: NodeId,
    to: NodeId,
    kind: &str,
) -> Option<String> {
    let conn = rusqlite::Connection::open(SqliteOverlayStore::db_path(overlay_dir)).unwrap();
    conn.query_row(
        "SELECT state FROM cross_links WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3",
        [from.to_string(), to.to_string(), kind.to_string()],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

fn promoted_audit_count(overlay_dir: &std::path::Path, from: NodeId, to: NodeId) -> usize {
    let conn = rusqlite::Connection::open(SqliteOverlayStore::db_path(overlay_dir)).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM cross_link_audit
         WHERE from_node = ?1 AND to_node = ?2 AND event_kind = 'promoted'",
        [from.to_string(), to.to_string()],
        |row| row.get::<_, i64>(0),
    )
    .unwrap() as usize
}

fn edge_exists(graph_dir: &std::path::Path, from: NodeId, to: NodeId) -> bool {
    let graph = SqliteGraphStore::open_existing(graph_dir).unwrap();
    graph
        .outbound(from, Some(EdgeKind::References))
        .unwrap()
        .iter()
        .any(|e| e.to == to)
}

#[test]
fn links_accept_commit_graph_insert_failure_leaves_pending_without_edge() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edge_id = curated_edge_id(from, to, EdgeKind::References);

    {
        let mut store = FailingStore {
            inner: RealLinksStore {
                graph: &mut graph,
                overlay: &mut overlay,
            },
            switches: FailureSwitches {
                insert_edge_fails_once: true,
                ..Default::default()
            },
        };
        let err = commands::links_accept_commit(
            &mut store,
            &CommitArgs {
                from,
                to,
                kind: OverlayEdgeKind::References,
                edge_kind: EdgeKind::References,
                edge_id,
                reviewer: "reviewer-a",
            },
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("insert_edge failure"),
            "expected Phase 2 injection error, got: {err}"
        );
    }
    drop(graph);

    let overlay_dir = synrepo_dir.join("overlay");
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("pending_promotion"),
        "overlay must be pending_promotion after Phase 2 failure"
    );
    assert!(
        !edge_exists(&synrepo_dir.join("graph"), from, to),
        "graph edge must not exist after Phase 2 failure"
    );
    assert_eq!(
        promoted_audit_count(&overlay_dir, from, to),
        0,
        "no promoted audit row should exist yet"
    );

    drop(overlay);
    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    assert!(edge_exists(&synrepo_dir.join("graph"), from, to));
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("promoted")
    );
    assert_eq!(promoted_audit_count(&overlay_dir, from, to), 1);
}

#[test]
fn links_accept_commit_overlay_finalize_failure_invokes_compensation() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edge_id = curated_edge_id(from, to, EdgeKind::References);

    {
        let mut store = FailingStore {
            inner: RealLinksStore {
                graph: &mut graph,
                overlay: &mut overlay,
            },
            switches: FailureSwitches {
                mark_promoted_fails_once: true,
                ..Default::default()
            },
        };
        let err = commands::links_accept_commit(
            &mut store,
            &CommitArgs {
                from,
                to,
                kind: OverlayEdgeKind::References,
                edge_kind: EdgeKind::References,
                edge_id,
                reviewer: "reviewer-a",
            },
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("overlay finalize failed"),
            "expected overlay finalize error, got: {err}"
        );
        assert!(
            err.to_string().contains("mark_promoted failure"),
            "original overlay error must be preserved in message, got: {err}"
        );
    }
    drop(graph);
    drop(overlay);

    let overlay_dir = synrepo_dir.join("overlay");
    assert!(
        !edge_exists(&synrepo_dir.join("graph"), from, to),
        "compensation must have removed the graph edge"
    );
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("pending_promotion"),
        "overlay must still be pending_promotion (Phase 3 never completed)"
    );

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edges: Vec<_> = graph
        .outbound(from, Some(EdgeKind::References))
        .unwrap()
        .into_iter()
        .filter(|e| e.to == to)
        .collect();
    assert_eq!(edges.len(), 1, "retry must not produce duplicate edges");
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("promoted")
    );
    assert_eq!(
        promoted_audit_count(&overlay_dir, from, to),
        1,
        "exactly one promotion audit row after rollback + retry"
    );
}

#[test]
fn links_accept_commit_both_failures_surface_original_error_and_inconsistency() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edge_id = curated_edge_id(from, to, EdgeKind::References);

    {
        let mut store = FailingStore {
            inner: RealLinksStore {
                graph: &mut graph,
                overlay: &mut overlay,
            },
            switches: FailureSwitches {
                mark_promoted_fails_once: true,
                delete_edge_fails_once: true,
                ..Default::default()
            },
        };
        let err = commands::links_accept_commit(
            &mut store,
            &CommitArgs {
                from,
                to,
                kind: OverlayEdgeKind::References,
                edge_kind: EdgeKind::References,
                edge_id,
                reviewer: "reviewer-a",
            },
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("overlay finalize failed"),
            "error must surface original overlay failure, got: {err}"
        );
        assert!(
            err.to_string().contains("mark_promoted failure"),
            "original overlay error text must be present, got: {err}"
        );
        assert!(
            !err.to_string().contains("delete_edge"),
            "compensation error must not mask original error, got: {err}"
        );
    }

    let overlay_dir = synrepo_dir.join("overlay");
    assert!(
        edge_exists(&synrepo_dir.join("graph"), from, to),
        "graph edge persists because compensation failed"
    );
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("pending_promotion"),
        "overlay state must NOT be promoted - this is the divergence signal"
    );
}

#[test]
fn links_accept_commit_rollback_then_retry_leaves_single_promoted_audit() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edge_id = curated_edge_id(from, to, EdgeKind::References);

    {
        let mut store = FailingStore {
            inner: RealLinksStore {
                graph: &mut graph,
                overlay: &mut overlay,
            },
            switches: FailureSwitches {
                mark_promoted_fails_once: true,
                ..Default::default()
            },
        };
        let _err = commands::links_accept_commit(
            &mut store,
            &CommitArgs {
                from,
                to,
                kind: OverlayEdgeKind::References,
                edge_kind: EdgeKind::References,
                edge_id,
                reviewer: "reviewer-a",
            },
        )
        .unwrap_err();
    }
    drop(graph);
    drop(overlay);

    let overlay_dir = synrepo_dir.join("overlay");
    assert_eq!(
        promoted_audit_count(&overlay_dir, from, to),
        0,
        "no promoted audit row yet - Phase 3 never committed"
    );

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    assert_eq!(
        promoted_audit_count(&overlay_dir, from, to),
        1,
        "exactly one promotion audit row after retry"
    );
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("promoted")
    );

    // Third accept must be an idempotent no-op on the promoted-audit count.
    commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();
    assert_eq!(
        promoted_audit_count(&overlay_dir, from, to),
        1,
        "idempotent replay must not append a second promoted audit"
    );
}
