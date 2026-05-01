//! Shared fixtures and helpers for the split `compiler::tests` modules.

use super::super::test_support::bootstrap;
use super::super::GraphCardCompiler;
use crate::{
    config::Config,
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    overlay::OverlayStore,
    pipeline::git::test_support::{git, init_commit},
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::EdgeKind,
};
use parking_lot::Mutex;
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;
use time::OffsetDateTime;

pub(super) fn make_compiler(
    graph: SqliteGraphStore,
    repo: &tempfile::TempDir,
) -> GraphCardCompiler {
    GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
}

pub(crate) fn fresh_symbol_fixture() -> (tempfile::TempDir, SqliteGraphStore, SymbolNodeId) {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "/// Docs.\npub fn annotated() -> u32 { 7 }\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let sym_edge = graph
        .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let sym_id = match sym_edge.to {
        NodeId::Symbol(id) => id,
        _ => panic!("expected symbol"),
    };
    (repo, graph, sym_id)
}

pub(super) fn current_content_hash(graph: &SqliteGraphStore, path: &str) -> String {
    graph.file_by_path(path).unwrap().unwrap().content_hash
}

pub(crate) fn make_overlay_store(repo: &tempfile::TempDir) -> Arc<Mutex<dyn OverlayStore>> {
    let overlay_dir = repo.path().join(".synrepo/overlay");
    let store = SqliteOverlayStore::open(&overlay_dir).unwrap();
    Arc::new(Mutex::new(store))
}

pub(super) fn sample_proposed_link(
    from: NodeId,
    to: NodeId,
    from_hash: &str,
    to_hash: &str,
    tier: crate::overlay::ConfidenceTier,
) -> crate::overlay::OverlayLink {
    crate::overlay::OverlayLink {
        from,
        to,
        kind: crate::overlay::OverlayEdgeKind::References,
        epistemic: crate::overlay::OverlayEpistemic::MachineAuthoredHighConf,
        source_spans: vec![crate::overlay::CitedSpan {
            artifact: from,
            normalized_text: "span".into(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        target_spans: vec![crate::overlay::CitedSpan {
            artifact: to,
            normalized_text: "span".into(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        from_content_hash: from_hash.into(),
        to_content_hash: to_hash.into(),
        confidence_score: match tier {
            crate::overlay::ConfidenceTier::High => 0.9,
            crate::overlay::ConfidenceTier::ReviewQueue => 0.7,
            crate::overlay::ConfidenceTier::BelowThreshold => 0.4,
        },
        confidence_tier: tier,
        rationale: None,
        provenance: crate::overlay::CrossLinkProvenance {
            pass_id: "test".into(),
            model_identity: "test".into(),
            generated_at: OffsetDateTime::now_utc(),
        },
    }
}

/// Build a temp repo with a single `src/lib.rs` committed, returning the repo,
/// a `GraphCardCompiler` configured with `Config::default()`, and the file
/// + symbol IDs of the committed symbol.
pub(super) fn git_backed_fixture() -> (
    tempfile::TempDir,
    GraphCardCompiler,
    FileNodeId,
    SymbolNodeId,
) {
    let repo = tempdir().unwrap();
    init_commit(&repo);
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "/// Adds.\npub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();
    git(&repo, &["add", "src/lib.rs"]);
    git(&repo, &["commit", "-m", "add add"]);

    let graph = bootstrap(&repo);
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let file_id = file.id;
    let sym_edge = graph
        .outbound(NodeId::File(file_id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let sym_id = match sym_edge.to {
        NodeId::Symbol(id) => id,
        _ => panic!("expected symbol edge target"),
    };
    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_config(Config::default());
    (repo, compiler, file_id, sym_id)
}

pub(super) fn neighborhood_fixture() -> (
    tempfile::TempDir,
    GraphCardCompiler,
    FileNodeId,
    SymbolNodeId,
) {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "/// Entry point.\npub fn main_fn() -> u32 { helper() }\n\n/// Helper.\npub fn helper() -> u32 { 42 }\n",
    ).unwrap();
    fs::write(
        repo.path().join("src/utils.rs"),
        "pub fn util() -> u32 { 1 }\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let syms: Vec<_> = graph
        .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.to {
            NodeId::Symbol(sid) => {
                let s = graph.get_symbol(sid).ok()??;
                if s.display_name == "main_fn" {
                    Some(sid)
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();
    let sym_id = syms.into_iter().next().expect("main_fn must exist");

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    (repo, compiler, file.id, sym_id)
}

pub(super) fn multi_file_fixture() -> (tempfile::TempDir, GraphCardCompiler, FileNodeId) {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/utils.ts"),
        "export function helper() {}\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.ts"),
        "import { helper } from './utils';\nexport function main() { return helper(); }\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let main_id = graph.file_by_path("src/main.ts").unwrap().unwrap().id;
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    (repo, compiler, main_id)
}
