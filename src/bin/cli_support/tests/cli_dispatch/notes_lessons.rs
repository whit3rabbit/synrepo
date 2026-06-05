use super::super::super::cli_args::{Command, NotesCommand};
use super::parse;

#[test]
fn notes_add_dispatches_to_notes_variant() {
    let cli = parse(&[
        "notes",
        "add",
        "--target-kind",
        "path",
        "--target",
        "src/lib.rs",
        "--claim",
        "The file owns CLI dispatch.",
        "--json",
    ]);
    let Some(Command::Notes(NotesCommand::Add { json, .. })) = cli.command else {
        panic!("notes add should parse");
    };
    assert!(json);
}

#[test]
fn lesson_public_commands_parse() {
    let remember = parse(&[
        "remember",
        "Use overlay lessons for repo-scoped context.",
        "--target",
        "src/lib.rs",
        "--target-kind",
        "path",
        "--ttl",
        "30d",
        "--evidence",
        "Design approved.",
        "--json",
    ]);
    assert!(matches!(
        remember.command,
        Some(Command::Remember(args)) if args.json && args.ttl.as_deref() == Some("30d")
    ));

    let recall = parse(&["recall", "overlay lessons", "--limit", "3", "--json"]);
    assert!(matches!(
        recall.command,
        Some(Command::Recall(args)) if args.query == "overlay lessons" && args.limit == Some(3)
    ));

    let lessons = parse(&["lessons", "--include-hidden"]);
    assert!(matches!(
        lessons.command,
        Some(Command::Lessons(args)) if args.include_hidden
    ));

    let forget = parse(&["forget", "note_abc", "--reason", "obsolete", "--json"]);
    assert!(matches!(
        forget.command,
        Some(Command::Forget(args)) if args.lesson_id == "note_abc" && args.json
    ));

    let verify = parse(&["verify-lesson", "note_abc", "--actor", "tester"]);
    assert!(matches!(
        verify.command,
        Some(Command::VerifyLesson(args)) if args.actor == "tester"
    ));
}
