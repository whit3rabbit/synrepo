use std::fs;
use std::sync::{Mutex, OnceLock};

use super::{audit, cards, primitives, search, SynrepoState};
use crate::{
    bootstrap,
    config::Config,
    overlay::{
        CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic,
        OverlayLink, OverlayStore,
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::snapshot,
};
use tempfile::tempdir;
use time::OffsetDateTime;

static SNAPSHOT_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[test]
#[ignore = "snapshot parity uses the process-global ArcSwap<Graph>; run explicitly to avoid parallel-suite interference"]
fn migrated_read_tools_match_snapshot_and_sqlite_outputs() {
    let _env_guard = SNAPSHOT_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();

    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::create_dir_all(repo.path().join("tests")).unwrap();
    fs::create_dir_all(repo.path().join("docs/adr")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        r#"
pub fn helper() -> i32 { 1 }
pub fn add() -> i32 { helper() }
"#,
    )
    .unwrap();
    fs::write(
        repo.path().join("tests/lib_test.rs"),
        r#"
#[test]
fn add_smoke() {
    assert_eq!(synrepo::add(), 1);
}
"#,
    )
    .unwrap();
    fs::write(
        repo.path().join("docs/adr/add.md"),
        r#"---
title: Add Decision
governs:
  - src/lib.rs
---

Use the add helper.
"#,
    )
    .unwrap();

    bootstrap::bootstrap(repo.path(), None).unwrap();
    assert!(snapshot::current().snapshot_epoch > 0);

    let state = SynrepoState {
        config: Config::load(repo.path()).unwrap(),
        repo_root: repo.path().to_path_buf(),
    };

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();

    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let symbol = graph
        .all_symbol_names()
        .unwrap()
        .into_iter()
        .find(|(_, _, name)| name == "add" || name.ends_with("::add"))
        .map(|(id, _, _)| id)
        .unwrap();
    let related_file = graph.file_by_path("tests/lib_test.rs").unwrap().unwrap();

    overlay
        .insert_link(OverlayLink {
            from: crate::NodeId::File(file.id),
            to: crate::NodeId::File(related_file.id),
            kind: OverlayEdgeKind::References,
            epistemic: OverlayEpistemic::MachineAuthoredHighConf,
            source_spans: vec![CitedSpan {
                artifact: crate::NodeId::File(file.id),
                normalized_text: "pub fn add() -> i32 { helper() }".to_string(),
                verified_at_offset: 0,
                lcs_ratio: 1.0,
            }],
            target_spans: vec![CitedSpan {
                artifact: crate::NodeId::File(related_file.id),
                normalized_text: "assert_eq!(synrepo::add(), 1);".to_string(),
                verified_at_offset: 0,
                lcs_ratio: 1.0,
            }],
            from_content_hash: file.content_hash.clone(),
            to_content_hash: related_file.content_hash.clone(),
            confidence_score: 0.9,
            confidence_tier: ConfidenceTier::High,
            rationale: Some("test candidate".to_string()),
            provenance: CrossLinkProvenance {
                pass_id: "test-pass".to_string(),
                model_identity: "test-model".to_string(),
                generated_at: OffsetDateTime::now_utc(),
            },
        })
        .unwrap();

    let with_snapshot = collect_tool_outputs(&state, &file.path, symbol.to_string());

    std::env::set_var("SYNREPO_DISABLE_GRAPH_SNAPSHOT", "1");
    let with_sqlite = collect_tool_outputs(&state, &file.path, symbol.to_string());
    std::env::remove_var("SYNREPO_DISABLE_GRAPH_SNAPSHOT");

    assert_eq!(with_snapshot, with_sqlite);
}

fn collect_tool_outputs(
    state: &SynrepoState,
    file_path: &str,
    symbol_target: String,
) -> Vec<String> {
    vec![
        cards::handle_card(state, file_path.to_string(), "tiny".to_string()),
        cards::handle_entrypoints(state, None, "tiny".to_string()),
        cards::handle_module_card(state, "src".to_string(), "tiny".to_string()),
        cards::handle_public_api(state, "src".to_string(), "tiny".to_string()),
        cards::handle_minimum_context(state, file_path.to_string(), "normal".to_string()),
        cards::handle_call_path(state, symbol_target.clone(), "tiny".to_string()),
        cards::handle_test_surface(state, "src".to_string(), "tiny".to_string()),
        cards::handle_change_risk(state, file_path.to_string(), "tiny".to_string()),
        search::handle_where_to_edit(state, "update add behavior".to_string(), 5),
        search::handle_change_impact(state, file_path.to_string()),
        primitives::handle_node(state, symbol_target.clone()),
        primitives::handle_edges(state, symbol_target.clone(), "inbound".to_string(), None),
        primitives::handle_query(state, format!("outbound {symbol_target}")),
        primitives::handle_provenance(state, symbol_target.clone()),
        audit::handle_findings(
            state.repo_root.as_path(),
            Some(symbol_target),
            None,
            None,
            20,
        ),
    ]
}
