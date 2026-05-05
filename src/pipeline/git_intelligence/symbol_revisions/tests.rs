use crate::{
    config::Config,
    core::{
        ids::SymbolNodeId,
        provenance::{CreatedBy, Provenance, SourceRef},
    },
    pipeline::git::{
        test_support::{git, init_commit},
        GitIntelligenceContext,
    },
    pipeline::git_intelligence::derive_symbol_revisions,
    structure::graph::{Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode, Visibility},
};
use std::path::Path;
use tempfile::tempdir;
use time::OffsetDateTime;

fn test_provenance() -> Provenance {
    Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "deadbeef".to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: "test".to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: "lib.rs".to_string(),
            content_hash: "hash".to_string(),
        }],
    }
}

fn make_symbol(id: u64, file_id: u128, qname: &str, body_hash: &str) -> SymbolNode {
    SymbolNode {
        id: SymbolNodeId(id as u128),
        file_id: crate::core::ids::FileNodeId(file_id as u128),
        qualified_name: qname.to_string(),
        display_name: qname.split("::").last().unwrap_or(qname).to_string(),
        kind: SymbolKind::Function,
        visibility: Visibility::Public,
        body_byte_range: (0, 10),
        body_hash: body_hash.to_string(),
        signature: None,
        doc_comment: None,
        first_seen_rev: None,
        last_modified_rev: None,
        last_observed_rev: None,
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: test_provenance(),
    }
}

fn setup_graph() -> (
    crate::store::sqlite::SqliteGraphStore,
    crate::core::ids::FileNodeId,
) {
    let dir = tempfile::tempdir().unwrap();
    let graph_dir = dir.path().join("graph");
    let mut graph = crate::store::sqlite::SqliteGraphStore::open(&graph_dir).unwrap();

    let file_id = crate::core::ids::FileNodeId(1);
    graph
        .upsert_file(FileNode {
            id: file_id,
            root_id: "primary".to_string(),
            path: "lib.rs".to_string(),
            path_history: vec![],
            content_hash: "abc123".to_string(),
            content_sample_hashes: Vec::new(),
            size_bytes: 100,
            language: Some("rust".to_string()),
            inline_decisions: vec![],
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: test_provenance(),
        })
        .unwrap();

    // Keep tempdir alive for the test duration.
    std::mem::forget(dir);
    (graph, file_id)
}

fn current_hash_for(repo_path: &Path, filename: &str, symbol_name: &str) -> String {
    let content = std::fs::read(repo_path.join(filename)).unwrap();
    let output = crate::structure::parse::parse_file(Path::new(filename), &content)
        .unwrap()
        .unwrap();
    output
        .symbols
        .iter()
        .find(|s| s.qualified_name == symbol_name)
        .unwrap()
        .body_hash
        .clone()
}

#[test]
fn parse_symbols_extracts_body_hashes() {
    let content = b"fn hello() { }\nfn world() { }\n";
    let result = super::parse_symbols_for_hashes("test.rs", content);
    assert_eq!(result.len(), 2);
    let hello_key = ("hello".to_string(), "function".to_string());
    assert!(result.contains_key(&hello_key));
}

#[test]
fn parse_symbols_returns_empty_for_unsupported() {
    let result = super::parse_symbols_for_hashes("unknown.xyz", b"content");
    assert!(result.is_empty());
}

#[test]
fn body_hash_transition_produces_last_modified_rev() {
    let repo = tempdir().unwrap();
    init_commit(&repo);

    // Commit 1: original function body.
    std::fs::write(repo.path().join("lib.rs"), "fn foo() { old_body }\n").unwrap();
    git(&repo, &["add", "lib.rs"]);
    git(&repo, &["commit", "-m", "add foo with old body"]);

    // Commit 2: changed function body.
    std::fs::write(repo.path().join("lib.rs"), "fn foo() { new_body }\n").unwrap();
    git(&repo, &["add", "lib.rs"]);
    git(&repo, &["commit", "-m", "change foo body"]);

    let context = GitIntelligenceContext::inspect(repo.path(), &Config::default());
    let (mut graph, file_id) = setup_graph();

    let current_hash = current_hash_for(repo.path(), "lib.rs", "foo");
    let sym = make_symbol(1, file_id.0, "foo", &current_hash);
    graph.upsert_symbol(sym).unwrap();

    derive_symbol_revisions(repo.path(), &context, &mut graph, 50).unwrap();

    let updated = graph.get_symbol(SymbolNodeId(1)).unwrap().unwrap();
    assert!(
        updated.last_modified_rev.is_some(),
        "should detect the body hash change"
    );
    assert!(updated.first_seen_rev.is_some());
}

#[test]
fn no_hash_transition_returns_none_for_last_modified() {
    let repo = tempdir().unwrap();
    init_commit(&repo);

    std::fs::write(repo.path().join("lib.rs"), "fn bar() { body }\n").unwrap();
    git(&repo, &["add", "lib.rs"]);
    git(&repo, &["commit", "-m", "add bar"]);

    let context = GitIntelligenceContext::inspect(repo.path(), &Config::default());
    let (mut graph, file_id) = setup_graph();

    let current_hash = current_hash_for(repo.path(), "lib.rs", "bar");
    let sym = make_symbol(2, file_id.0, "bar", &current_hash);
    graph.upsert_symbol(sym).unwrap();

    derive_symbol_revisions(repo.path(), &context, &mut graph, 50).unwrap();

    let updated = graph.get_symbol(SymbolNodeId(2)).unwrap().unwrap();
    assert!(
        updated.last_modified_rev.is_none(),
        "no hash transition means no last_modified_rev"
    );
}

#[test]
fn new_symbol_in_history_gets_first_seen() {
    let repo = tempdir().unwrap();
    init_commit(&repo);

    std::fs::write(repo.path().join("lib.rs"), "fn old_fn() { }\n").unwrap();
    git(&repo, &["add", "lib.rs"]);
    git(&repo, &["commit", "-m", "initial lib"]);

    std::fs::write(
        repo.path().join("lib.rs"),
        "fn old_fn() { }\nfn new_fn() { }\n",
    )
    .unwrap();
    git(&repo, &["add", "lib.rs"]);
    git(&repo, &["commit", "-m", "add new_fn"]);

    let context = GitIntelligenceContext::inspect(repo.path(), &Config::default());
    let (mut graph, file_id) = setup_graph();

    let current_hash = current_hash_for(repo.path(), "lib.rs", "new_fn");
    let sym = make_symbol(3, file_id.0, "new_fn", &current_hash);
    graph.upsert_symbol(sym).unwrap();

    derive_symbol_revisions(repo.path(), &context, &mut graph, 50).unwrap();

    let updated = graph.get_symbol(SymbolNodeId(3)).unwrap().unwrap();
    assert!(
        updated.first_seen_rev.is_some(),
        "new_fn first_seen should be the commit that introduced it"
    );
}

#[test]
fn degraded_history_skips_derivation() {
    let repo = tempdir().unwrap();
    git(&repo, &["init"]);

    let context = GitIntelligenceContext::inspect(repo.path(), &Config::default());
    let (mut graph, file_id) = setup_graph();

    let sym = make_symbol(4, file_id.0, "degraded_fn", "hash");
    graph.upsert_symbol(sym).unwrap();

    derive_symbol_revisions(repo.path(), &context, &mut graph, 50).unwrap();

    let unchanged = graph.get_symbol(SymbolNodeId(4)).unwrap().unwrap();
    assert!(unchanged.first_seen_rev.is_none());
    assert!(unchanged.last_modified_rev.is_none());
}
