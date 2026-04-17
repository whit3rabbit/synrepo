use super::*;
use crate::config::Config;
use crate::pipeline::structural::run_structural_compile;
use crate::store::sqlite::SqliteGraphStore;
use crate::surface::card::compiler::test_support::bootstrap;
use crate::surface::card::compiler::GraphCardCompiler;
use crate::surface::card::Budget;
use insta::assert_snapshot;
use std::fs;
use tempfile::tempdir;

fn make_compiler(repo: &tempfile::TempDir) -> GraphCardCompiler {
    let graph = bootstrap(repo);
    GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
}

fn make_compiler_with_config(repo: &tempfile::TempDir, config: Config) -> GraphCardCompiler {
    let graph_dir = repo.path().join(".synrepo/graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();
    GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
}

// 4.1: tiny budget returns focal card + counts, no neighbor details
#[test]
fn tiny_budget_returns_focal_card_with_counts_and_no_details() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

    let compiler = make_compiler(&repo);
    let resp = resolve_neighborhood(&compiler, "src/lib.rs", Budget::Tiny).unwrap();

    assert_eq!(resp.budget, "tiny");
    assert!(resp.focal_card.is_object(), "focal_card must be present");
    assert!(resp.neighbors.is_none(), "no full cards at tiny budget");
    assert!(
        resp.neighbor_summaries.is_none(),
        "no summaries at tiny budget"
    );
    assert!(resp.decision_cards.is_none(), "no decisions at tiny budget");
    assert!(
        resp.co_change_partners.is_none(),
        "no partners at tiny budget"
    );
    // Overlay-only fields must be stripped from focal card.
    let fc = resp.focal_card.as_object().unwrap();
    assert!(!fc.contains_key("overlay_commentary"));
    assert!(!fc.contains_key("proposed_links"));
    assert!(!fc.contains_key("commentary_state"));
    assert!(!fc.contains_key("links_state"));
}

// 4.2: normal budget returns summaries (even if empty), not full cards; co-change missing without git
#[test]
fn normal_budget_returns_summaries_not_full_cards() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

    let compiler = make_compiler(&repo);
    let resp = resolve_neighborhood(&compiler, "src/lib.rs", Budget::Normal).unwrap();

    assert_eq!(resp.budget, "normal");
    assert!(
        resp.neighbor_summaries.is_some(),
        "summaries must be Some at normal budget"
    );
    assert!(
        resp.neighbors.is_none(),
        "full cards must be None at normal budget"
    );
    // No git init in tempdir → co-change state is missing.
    assert_eq!(resp.co_change_state, CoChangeState::Missing);
    assert_eq!(
        resp.co_change_partners.as_deref().unwrap_or(&[]).len(),
        0,
        "no co-change partners without git"
    );
}

// 4.3: deep budget returns full neighbor cards, not summaries
#[test]
fn deep_budget_returns_full_cards_not_summaries() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

    let compiler = make_compiler(&repo);
    let resp = resolve_neighborhood(&compiler, "src/lib.rs", Budget::Deep).unwrap();

    assert_eq!(resp.budget, "deep");
    assert!(
        resp.neighbors.is_some(),
        "full cards must be Some at deep budget"
    );
    assert!(
        resp.neighbor_summaries.is_none(),
        "summaries must be None at deep budget"
    );
}

// 4.4: unresolved target returns explicit error containing the target string
#[test]
fn unresolved_target_returns_error_with_target_string() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

    let compiler = make_compiler(&repo);
    let err = resolve_neighborhood(&compiler, "nonexistent_xyz_target", Budget::Tiny).unwrap_err();

    assert!(
        err.to_string().contains("nonexistent_xyz_target"),
        "error must include target string; got: {err}"
    );
}

// 4.5: missing git intelligence yields empty co-change list with state "missing"
#[test]
fn missing_git_intelligence_returns_empty_co_change_list() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
    // No git init → git cache returns None.

    let compiler = make_compiler(&repo);
    let resp = resolve_neighborhood(&compiler, "src/lib.rs", Budget::Normal).unwrap();

    assert_eq!(
        resp.co_change_state,
        CoChangeState::Missing,
        "state must be missing without git"
    );
    assert!(
        resp.co_change_partners
            .as_deref()
            .is_none_or(|p| p.is_empty()),
        "co_change_partners must be empty without git"
    );
}

// 4.6: governing decisions surface as DecisionCards at normal and deep budgets
#[test]
fn governing_decisions_included_in_normal_and_deep() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::create_dir_all(repo.path().join("docs/adr")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
    fs::write(
        repo.path().join("docs/adr/0001.md"),
        "---\ntitle: Use modular design\ngoverns: [src/lib.rs]\n---\n\nThis governs the library root.\n",
    )
    .unwrap();

    let config = Config {
        concept_directories: vec!["docs/adr".to_string()],
        ..Config::default()
    };
    let compiler = make_compiler_with_config(&repo, config);

    let resp = resolve_neighborhood(&compiler, "src/lib.rs", Budget::Normal).unwrap();
    let cards = resp
        .decision_cards
        .expect("decision_cards must be Some when a concept governs the target");
    assert!(
        !cards.is_empty(),
        "at least one decision card must be returned"
    );

    // Deep budget: open a second connection to the already-populated graph.
    let graph_dir = repo.path().join(".synrepo/graph");
    let graph2 = SqliteGraphStore::open(&graph_dir).unwrap();
    let compiler2 = GraphCardCompiler::new(Box::new(graph2), Some(repo.path()));
    let resp2 = resolve_neighborhood(&compiler2, "src/lib.rs", Budget::Deep).unwrap();
    assert!(
        resp2
            .decision_cards
            .as_deref()
            .is_some_and(|c| !c.is_empty()),
        "decision cards must appear at deep budget too"
    );
}

// 4.7: snapshot the full MinimumContextResponse at each budget tier
#[test]
fn minimum_context_response_snapshots() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "/// Add two integers.\npub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let compiler = make_compiler(&repo);

    let tiny = serde_json::to_string_pretty(
        &resolve_neighborhood(&compiler, "src/lib.rs", Budget::Tiny).unwrap(),
    )
    .unwrap();
    assert_snapshot!("neighborhood_response_tiny", tiny);

    let normal = serde_json::to_string_pretty(
        &resolve_neighborhood(&compiler, "src/lib.rs", Budget::Normal).unwrap(),
    )
    .unwrap();
    assert_snapshot!("neighborhood_response_normal", normal);

    let deep = serde_json::to_string_pretty(
        &resolve_neighborhood(&compiler, "src/lib.rs", Budget::Deep).unwrap(),
    )
    .unwrap();
    assert_snapshot!("neighborhood_response_deep", deep);
}

#[test]
fn co_change_state_serializes_as_snake_case() {
    let available = serde_json::to_string(&CoChangeState::Available).unwrap();
    assert_eq!(available, "\"available\"");
    let missing = serde_json::to_string(&CoChangeState::Missing).unwrap();
    assert_eq!(missing, "\"missing\"");
}

#[test]
fn strip_overlay_fields_removes_expected_keys() {
    let mut json = serde_json::json!({
        "name": "test",
        "overlay_commentary": "should be removed",
        "proposed_links": [],
        "commentary_state": "should be removed",
        "links_state": "should be removed",
        "commentary_text": "should be removed",
    });
    // Need to access the function from resolve module.
    use super::resolve::strip_overlay_fields;
    strip_overlay_fields(&mut json);
    let obj = json.as_object().unwrap();
    assert!(obj.contains_key("name"));
    assert!(!obj.contains_key("overlay_commentary"));
    assert!(!obj.contains_key("proposed_links"));
    assert!(!obj.contains_key("commentary_state"));
    assert!(!obj.contains_key("links_state"));
    assert!(!obj.contains_key("commentary_text"));
}

#[test]
fn co_change_partner_has_correct_labels() {
    let partner = CoChangePartner {
        path: "src/main.rs".to_string(),
        co_change_count: 5,
        source: "git_intelligence",
        granularity: "file",
    };
    let json = serde_json::to_value(&partner).unwrap();
    assert_eq!(json["source"], "git_intelligence");
    assert_eq!(json["granularity"], "file");
}

#[test]
fn edge_counts_serializes_with_expected_keys() {
    let counts = EdgeCounts {
        outbound_calls_count: 3,
        outbound_imports_count: 1,
        governs_count: 2,
        co_change_count: 4,
    };
    let json = serde_json::to_value(&counts).unwrap();
    assert_eq!(json["outbound_calls_count"], 3);
    assert_eq!(json["outbound_imports_count"], 1);
    assert_eq!(json["governs_count"], 2);
    assert_eq!(json["co_change_count"], 4);
}
