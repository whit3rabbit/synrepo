use tempfile::tempdir;

use super::super::commands::{notes_add_output, notes_list_output};
use super::support::seed_graph;

#[test]
fn notes_cli_json_labels_overlay_advisory_records() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());

    let add = notes_add_output(
        repo.path(),
        "symbol",
        &ids.symbol_id.to_string(),
        "This symbol is safe to edit after checking callers.",
        "codex-test",
        "medium",
        None,
        None,
        None,
        true,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&add).unwrap();
    assert_eq!(value["source_store"], "overlay");
    assert_eq!(value["advisory"], true);
    assert_eq!(value["status"], "unverified");

    let list = notes_list_output(
        repo.path(),
        Some("symbol"),
        Some(&ids.symbol_id.to_string()),
        Some(10),
        false,
        true,
    )
    .unwrap();
    let notes: serde_json::Value = serde_json::from_str(&list).unwrap();
    assert_eq!(notes.as_array().unwrap().len(), 1);
    assert_eq!(notes[0]["source_store"], "overlay");
    assert_eq!(notes[0]["advisory"], true);
}
