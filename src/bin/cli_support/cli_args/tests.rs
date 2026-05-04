use super::{Cli, Command};
use clap::Parser;

#[test]
fn init_force_flag_defaults_off_and_sets_on_request() {
    let default = Cli::try_parse_from(["synrepo", "init"]).unwrap();
    assert!(matches!(
        default.command,
        Some(Command::Init { force: false, .. })
    ));

    let forced = Cli::try_parse_from(["synrepo", "init", "--force"]).unwrap();
    assert!(matches!(
        forced.command,
        Some(Command::Init { force: true, .. })
    ));
}

#[test]
fn task_route_parses_task_path_and_json() {
    let parsed = Cli::try_parse_from([
        "synrepo",
        "task-route",
        "convert var to const",
        "--path",
        "src/app.ts",
        "--json",
    ])
    .unwrap();

    assert!(matches!(
        parsed.command,
        Some(Command::TaskRoute { task, path, json })
            if task == "convert var to const" && path.as_deref() == Some("src/app.ts") && json
    ));
}
