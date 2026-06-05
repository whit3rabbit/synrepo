use tempfile::tempdir;

use crate::cli_support::{
    cli_args::{LessonListArgs, LessonRecallArgs, LessonRememberArgs},
    commands::{lesson_list_output, lesson_recall_output, lesson_remember_output},
};
use synrepo::bootstrap::bootstrap;

#[test]
fn cli_lessons_remember_recall_and_list() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("lib.rs"), "pub fn remembered() {}\n").unwrap();
    bootstrap(repo.path(), None, false).unwrap();

    let remembered = lesson_remember_output(
        repo.path(),
        LessonRememberArgs {
            claim: "The CLI lesson path is overlay only.".to_string(),
            target: None,
            target_kind: None,
            ttl: Some("1d".to_string()),
            evidence: vec!["implementation note".to_string()],
            actor: "test".to_string(),
            confidence: "medium".to_string(),
            json: true,
        },
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&remembered).unwrap();
    assert_eq!(value["target_kind"], "repo");
    assert_eq!(value["source_store"], "overlay");
    assert_eq!(value["advisory"], true);

    let recalled = lesson_recall_output(
        repo.path(),
        LessonRecallArgs {
            query: "overlay".to_string(),
            target: None,
            target_kind: None,
            limit: Some(5),
            include_hidden: false,
            json: true,
        },
    )
    .unwrap();
    let lessons: serde_json::Value = serde_json::from_str(&recalled).unwrap();
    assert_eq!(lessons.as_array().unwrap().len(), 1);

    let listed = lesson_list_output(
        repo.path(),
        LessonListArgs {
            target: None,
            target_kind: None,
            limit: Some(5),
            include_hidden: false,
            json: false,
        },
    )
    .unwrap();
    assert!(listed.contains("Found 1 lessons."));
}
