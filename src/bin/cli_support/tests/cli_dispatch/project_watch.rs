use super::super::super::cli_args::{Command, ProjectCommand, WatchCommand};
use super::parse;

#[test]
fn project_subcommands_parse() {
    let add = parse(&["project", "add", "/tmp/app"]);
    assert!(matches!(
        add.command,
        Some(Command::Project(ProjectCommand::Add { .. }))
    ));

    let list = parse(&["project", "list", "--json"]);
    assert!(matches!(
        list.command,
        Some(Command::Project(ProjectCommand::List { json: true }))
    ));

    let inspect = parse(&["project", "inspect", "/tmp/app", "--json"]);
    assert!(matches!(
        inspect.command,
        Some(Command::Project(ProjectCommand::Inspect { json: true, .. }))
    ));

    let remove = parse(&["project", "remove", "/tmp/app"]);
    assert!(matches!(
        remove.command,
        Some(Command::Project(ProjectCommand::Remove { .. }))
    ));

    let prune = parse(&["project", "prune-missing", "--apply", "--json"]);
    assert!(matches!(
        prune.command,
        Some(Command::Project(ProjectCommand::PruneMissing {
            apply: true,
            json: true,
        }))
    ));

    let use_cmd = parse(&["project", "use", "proj_abc"]);
    assert!(matches!(
        use_cmd.command,
        Some(Command::Project(ProjectCommand::Use { .. }))
    ));

    let rename = parse(&["project", "rename", "proj_abc", "work-app"]);
    assert!(matches!(
        rename.command,
        Some(Command::Project(ProjectCommand::Rename { .. }))
    ));
}

#[test]
fn watch_daemon_and_no_ui_are_distinct_flags() {
    let daemon = parse(&["watch", "--daemon"]);
    let Some(Command::Watch {
        daemon,
        no_ui,
        command,
    }) = daemon.command
    else {
        panic!("watch --daemon should parse");
    };
    assert!(daemon);
    assert!(!no_ui);
    assert!(command.is_none());

    let no_ui = parse(&["watch", "--no-ui"]);
    let Some(Command::Watch {
        daemon,
        no_ui,
        command,
    }) = no_ui.command
    else {
        panic!("watch --no-ui should parse");
    };
    assert!(!daemon);
    assert!(no_ui);
    assert!(command.is_none());
}

#[test]
fn watch_status_and_stop_parse_as_watch_subcommands() {
    let status = parse(&["watch", "status"]);
    let Some(Command::Watch {
        command: Some(WatchCommand::Status),
        ..
    }) = status.command
    else {
        panic!("watch status should parse to WatchCommand::Status");
    };

    let stop = parse(&["watch", "stop"]);
    let Some(Command::Watch {
        command: Some(WatchCommand::Stop),
        ..
    }) = stop.command
    else {
        panic!("watch stop should parse to WatchCommand::Stop");
    };
}
