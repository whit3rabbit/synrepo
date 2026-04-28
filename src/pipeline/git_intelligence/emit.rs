//! Convert computed co-change data into persisted graph edges.

use std::collections::HashMap;

use crate::core::ids::{FileNodeId, NodeId};
use crate::core::provenance::{CreatedBy, Provenance, SourceRef};
use crate::pipeline::structural::derive_edge_id;
use crate::structure::graph::{Edge, EdgeKind, Epistemic, GraphStore};

use super::types::{GitCoChange, GitHistoryInsights};

/// Minimum co-change count to emit an edge. A single co-occurrence is noise;
/// require at least 2 sampled commits touching both files.
const COCHANGE_MIN_COUNT: usize = 2;

/// Emit `CoChangesWith` edges from the computed co-change pairs.
///
/// For each pair in `insights.co_changes` with `co_change_count >=
/// COCHANGE_MIN_COUNT`, resolves both paths to `FileNodeId` via `file_index`,
/// derives a deterministic edge ID, and inserts the edge with
/// `Epistemic::GitObserved` authority.
///
/// Returns the number of edges emitted. Pairs where either path is not in
/// `file_index` are silently skipped (file may have been deleted or not yet
/// discovered).
pub fn emit_cochange_edges(
    graph: &mut dyn GraphStore,
    insights: &GitHistoryInsights,
    file_index: &HashMap<String, FileNodeId>,
    revision: &str,
) -> crate::Result<usize> {
    let mut count = 0usize;
    for GitCoChange {
        left_path,
        right_path,
        co_change_count,
    } in &insights.co_changes
    {
        if *co_change_count < COCHANGE_MIN_COUNT {
            continue;
        }
        let Some(left_id) = file_index.get(left_path) else {
            continue;
        };
        let Some(right_id) = file_index.get(right_path) else {
            continue;
        };

        let edge_id = derive_edge_id(
            NodeId::File(*left_id),
            NodeId::File(*right_id),
            EdgeKind::CoChangesWith,
        );
        graph.insert_edge(Edge {
            id: edge_id,
            from: NodeId::File(*left_id),
            to: NodeId::File(*right_id),
            kind: EdgeKind::CoChangesWith,
            owner_file_id: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::GitObserved,
            drift_score: 0.0,
            provenance: Provenance {
                created_at: time::OffsetDateTime::now_utc(),
                source_revision: revision.to_string(),
                created_by: CreatedBy::StructuralPipeline,
                pass: "stage5_cochange".to_string(),
                source_artifacts: vec![
                    SourceRef {
                        file_id: Some(*left_id),
                        path: left_path.clone(),
                        content_hash: String::new(),
                    },
                    SourceRef {
                        file_id: Some(*right_id),
                        path: right_path.clone(),
                        content_hash: String::new(),
                    },
                ],
            },
        })?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::core::ids::FileNodeId;
    use crate::core::provenance::Provenance;
    use crate::pipeline::git::GitIntelligenceReadiness;
    use crate::pipeline::git_intelligence::types::{
        GitCoChange, GitHistoryInsights, GitHistorySample, GitIntelligenceStatus,
    };
    use crate::structure::graph::{EdgeKind, Epistemic, GraphStore};
    use tempfile::tempdir;

    use super::emit_cochange_edges;

    /// Minimal helper: build insights with the given co-change pairs.
    fn make_insights(co_changes: Vec<GitCoChange>) -> GitHistoryInsights {
        GitHistoryInsights {
            history: GitHistorySample {
                status: GitIntelligenceStatus {
                    source_revision: "abc123".to_string(),
                    requested_commit_depth: 100,
                    readiness: GitIntelligenceReadiness::Ready,
                },
                commits: vec![],
            },
            hotspots: vec![],
            ownership: vec![],
            co_changes,
        }
    }

    #[test]
    fn emits_edges_for_pairs_above_threshold() {
        let repo = tempdir().unwrap();
        let graph_dir = repo.path().join(".synrepo/graph");
        let mut store = crate::store::sqlite::SqliteGraphStore::open(&graph_dir).unwrap();

        // Insert two file nodes so outbound queries work.
        let file_a = crate::structure::graph::FileNode {
            id: FileNodeId(1),
            root_id: "primary".to_string(),
            path: "src/a.rs".to_string(),
            path_history: vec![],
            content_hash: "a".to_string(),
            size_bytes: 10,
            language: Some("rust".to_string()),
            inline_decisions: vec![],
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: Provenance::structural("test", "rev1", vec![]),
        };
        let file_b = crate::structure::graph::FileNode {
            id: FileNodeId(2),
            root_id: "primary".to_string(),
            path: "src/b.rs".to_string(),
            path_history: vec![],
            content_hash: "b".to_string(),
            size_bytes: 10,
            language: Some("rust".to_string()),
            inline_decisions: vec![],
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: Provenance::structural("test", "rev1", vec![]),
        };
        store.upsert_file(file_a.clone()).unwrap();
        store.upsert_file(file_b.clone()).unwrap();

        let mut file_index = HashMap::new();
        file_index.insert("src/a.rs".to_string(), file_a.id);
        file_index.insert("src/b.rs".to_string(), file_b.id);

        let insights = make_insights(vec![GitCoChange {
            left_path: "src/a.rs".to_string(),
            right_path: "src/b.rs".to_string(),
            co_change_count: 3,
        }]);

        let count = emit_cochange_edges(&mut store, &insights, &file_index, "rev1").unwrap();
        assert_eq!(count, 1);

        let edges = store
            .outbound(
                crate::core::ids::NodeId::File(file_a.id),
                Some(EdgeKind::CoChangesWith),
            )
            .unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].epistemic, Epistemic::GitObserved);
    }

    #[test]
    fn filters_pairs_below_threshold() {
        let repo = tempdir().unwrap();
        let graph_dir = repo.path().join(".synrepo/graph");
        let mut store = crate::store::sqlite::SqliteGraphStore::open(&graph_dir).unwrap();

        let file_a = crate::structure::graph::FileNode {
            id: FileNodeId(1),
            root_id: "primary".to_string(),
            path: "src/a.rs".to_string(),
            path_history: vec![],
            content_hash: "a".to_string(),
            size_bytes: 10,
            language: Some("rust".to_string()),
            inline_decisions: vec![],
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: Provenance::structural("test", "rev1", vec![]),
        };
        let file_b = crate::structure::graph::FileNode {
            id: FileNodeId(2),
            root_id: "primary".to_string(),
            path: "src/b.rs".to_string(),
            path_history: vec![],
            content_hash: "b".to_string(),
            size_bytes: 10,
            language: Some("rust".to_string()),
            inline_decisions: vec![],
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: Provenance::structural("test", "rev1", vec![]),
        };
        store.upsert_file(file_a.clone()).unwrap();
        store.upsert_file(file_b.clone()).unwrap();

        let mut file_index = HashMap::new();
        file_index.insert("src/a.rs".to_string(), file_a.id);
        file_index.insert("src/b.rs".to_string(), file_b.id);

        // Count of 1 is below the threshold of 2.
        let insights = make_insights(vec![GitCoChange {
            left_path: "src/a.rs".to_string(),
            right_path: "src/b.rs".to_string(),
            co_change_count: 1,
        }]);

        let count = emit_cochange_edges(&mut store, &insights, &file_index, "rev1").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn skips_missing_paths() {
        let repo = tempdir().unwrap();
        let graph_dir = repo.path().join(".synrepo/graph");
        let mut store = crate::store::sqlite::SqliteGraphStore::open(&graph_dir).unwrap();

        let file_a = crate::structure::graph::FileNode {
            id: FileNodeId(1),
            root_id: "primary".to_string(),
            path: "src/a.rs".to_string(),
            path_history: vec![],
            content_hash: "a".to_string(),
            size_bytes: 10,
            language: Some("rust".to_string()),
            inline_decisions: vec![],
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: Provenance::structural("test", "rev1", vec![]),
        };
        store.upsert_file(file_a.clone()).unwrap();

        let mut file_index = HashMap::new();
        file_index.insert("src/a.rs".to_string(), file_a.id);
        // src/b.rs is intentionally absent from the index.

        let insights = make_insights(vec![GitCoChange {
            left_path: "src/a.rs".to_string(),
            right_path: "src/b.rs".to_string(),
            co_change_count: 5,
        }]);

        let count = emit_cochange_edges(&mut store, &insights, &file_index, "rev1").unwrap();
        assert_eq!(count, 0);
    }
}
