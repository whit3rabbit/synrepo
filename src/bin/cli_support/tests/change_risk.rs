use super::super::commands::change_risk_output;
use super::support::seed_graph;
use tempfile::tempdir;

#[test]
fn change_risk_output_json() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let out = change_risk_output(repo.path(), "src/lib.rs", Some("tiny"), true).unwrap();
    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(json["target_name"], "src/lib.rs");
    assert!(json["risk_score"].is_number());
    assert!(json["risk_level"].is_string());
}

#[test]
fn change_risk_output_human() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let out = change_risk_output(repo.path(), "src/lib.rs", Some("tiny"), false).unwrap();
    assert!(out.contains("Change Risk:"));
    assert!(out.contains("src/lib.rs"));
    assert!(out.contains("Risk level:"));
    assert!(out.contains("Risk score:"));
}

#[test]
fn change_risk_symbol_target() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let out = change_risk_output(repo.path(), "synrepo::lib", Some("tiny"), false).unwrap();
    assert!(out.contains("synrepo::lib"));
}
