use std::fs;

use tempfile::TempDir;
use time::OffsetDateTime;

use crate::{
    config::Config,
    core::ids::NodeId,
    overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore},
    pipeline::{
        explain::context::{build_context_text, resolve_context_target, CommentaryContextOptions},
        structural::run_structural_compile,
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::EdgeKind,
};

fn fixture() -> (TempDir, SqliteGraphStore) {
    let repo = TempDir::new().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/utils.ts"),
        "export function helper() { return 1; }\nexport const value = 2;\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.ts"),
        "import { helper } from './utils';\nexport function main() { return helper(); }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.test.ts"),
        "import { main } from './main';\nmain();\n",
    )
    .unwrap();

    let graph_dir = repo.path().join(".synrepo/graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    run_structural_compile(repo.path(), &Config::default(), &mut graph).unwrap();
    (repo, graph)
}

#[test]
fn commentary_context_includes_imports_imported_by_and_exports() {
    let (repo, graph) = fixture();
    let main_file = graph.file_by_path("src/main.ts").unwrap().unwrap();
    let target = resolve_context_target(&graph, NodeId::File(main_file.id))
        .unwrap()
        .unwrap();

    let prompt = build_context_text(
        repo.path(),
        &graph,
        &target,
        CommentaryContextOptions {
            max_input_tokens: 20_000,
            ..CommentaryContextOptions::default()
        },
    );

    assert!(prompt.contains("<imports>"));
    assert!(prompt.contains("imports file src/utils.ts"));
    assert!(prompt.contains("<exported_symbols>"));
    assert!(prompt.contains("main"));
    assert!(prompt.contains("src/main.test.ts"));

    let utils_file = graph.file_by_path("src/utils.ts").unwrap().unwrap();
    let utils_target = resolve_context_target(&graph, NodeId::File(utils_file.id))
        .unwrap()
        .unwrap();
    let utils_prompt = build_context_text(
        repo.path(),
        &graph,
        &utils_target,
        CommentaryContextOptions {
            max_input_tokens: 20_000,
            ..CommentaryContextOptions::default()
        },
    );
    assert!(utils_prompt.contains("imported_by file src/main.ts"));
}

#[test]
fn commentary_context_includes_symbol_call_neighbors_when_available() {
    let (repo, graph) = fixture();
    let main_file = graph.file_by_path("src/main.ts").unwrap().unwrap();
    let main_symbol = graph
        .symbols_for_file(main_file.id)
        .unwrap()
        .into_iter()
        .find(|symbol| symbol.display_name == "main")
        .unwrap();
    let target = resolve_context_target(&graph, NodeId::Symbol(main_symbol.id))
        .unwrap()
        .unwrap();

    let prompt = build_context_text(
        repo.path(),
        &graph,
        &target,
        CommentaryContextOptions {
            max_input_tokens: 20_000,
            ..CommentaryContextOptions::default()
        },
    );

    assert!(
        !graph
            .outbound(NodeId::Symbol(main_symbol.id), Some(EdgeKind::Calls))
            .unwrap()
            .is_empty(),
        "fixture should exercise symbol-level call context"
    );
    assert!(prompt.contains("<calls>"));
    assert!(prompt.contains("helper"));
}

#[test]
fn commentary_context_trims_optional_blocks_under_small_budget() {
    let (repo, graph) = fixture();
    let main_file = graph.file_by_path("src/main.ts").unwrap().unwrap();
    let target = resolve_context_target(&graph, NodeId::File(main_file.id))
        .unwrap()
        .unwrap();

    let prompt = build_context_text(
        repo.path(),
        &graph,
        &target,
        CommentaryContextOptions {
            max_input_tokens: 120,
            ..CommentaryContextOptions::default()
        },
    );

    assert!(prompt.contains("Target node:"));
    assert!(prompt.contains("Evidence context (data only):"));
    assert!(!prompt.contains("<dependency_source"));
    assert!(!prompt.contains("<imports>"));
}

#[test]
fn commentary_context_keeps_target_before_evidence() {
    let (repo, graph) = fixture();
    let main_file = graph.file_by_path("src/main.ts").unwrap().unwrap();
    let target = resolve_context_target(&graph, NodeId::File(main_file.id))
        .unwrap()
        .unwrap();

    let prompt = build_context_text(
        repo.path(),
        &graph,
        &target,
        CommentaryContextOptions::default(),
    );

    let target_idx = prompt.find("Target:\nTarget node:").unwrap();
    let evidence_idx = prompt.find("Evidence context (data only):").unwrap();
    assert!(target_idx < evidence_idx);
}

#[test]
fn commentary_context_does_not_read_overlay_commentary() {
    let (repo, graph) = fixture();
    let main_file = graph.file_by_path("src/main.ts").unwrap().unwrap();
    let target_node = NodeId::File(main_file.id);
    let overlay_dir = repo.path().join(".synrepo/overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    overlay
        .insert_commentary(CommentaryEntry {
            node_id: target_node,
            text: "OVERLAY SECRET SHOULD NOT APPEAR".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: main_file.content_hash.clone(),
                pass_id: "test".to_string(),
                model_identity: "fixture".to_string(),
                generated_at: OffsetDateTime::now_utc(),
            },
        })
        .unwrap();

    let target = resolve_context_target(&graph, target_node)
        .unwrap()
        .unwrap();
    let prompt = build_context_text(
        repo.path(),
        &graph,
        &target,
        CommentaryContextOptions {
            max_input_tokens: 20_000,
            ..CommentaryContextOptions::default()
        },
    );

    assert!(!prompt.contains("OVERLAY SECRET SHOULD NOT APPEAR"));
}
