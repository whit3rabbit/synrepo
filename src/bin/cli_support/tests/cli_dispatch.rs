//! CLI dispatch smoke tests.
//!
//! Pin the clap-level parse for every shipped subcommand so a future refactor
//! that reorders, renames, or accidentally strips a subcommand fails loud
//! without bringing up the runtime.

use clap::{CommandFactory, Parser};
use tempfile::tempdir;

use super::super::cli_args::{
    BenchCommand, Cli, Command, NotesCommand, ProjectCommand, StatsCommand, WatchCommand,
};
use super::super::entry::bare_ready_summary;

fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["synrepo"];
    full.extend_from_slice(args);
    Cli::try_parse_from(full).expect("args should parse")
}

#[test]
fn bare_synrepo_has_no_subcommand() {
    let cli = parse(&[]);
    assert!(
        cli.command.is_none(),
        "bare synrepo must leave Command unset so the router can take over"
    );
}

#[test]
fn bare_ready_summary_returns_error_when_synrepo_is_missing() {
    let dir = tempdir().unwrap();
    let err = bare_ready_summary(dir.path());
    assert!(
        err.is_err(),
        "status summary must surface the missing .synrepo error"
    );
}

#[test]
fn init_dispatches_to_init_variant() {
    let cli = parse(&["init"]);
    matches!(cli.command, Some(Command::Init { .. }))
        .then_some(())
        .expect("init should parse to Command::Init");
}

#[test]
fn status_dispatches_to_status_variant() {
    let cli = parse(&["status"]);
    matches!(cli.command, Some(Command::Status { .. }))
        .then_some(())
        .expect("status should parse to Command::Status");
}

#[test]
fn status_json_flag_round_trips() {
    let cli = parse(&["status", "--json"]);
    let Some(Command::Status { json, .. }) = cli.command else {
        panic!("status --json should parse to Command::Status");
    };
    assert!(json, "--json must flip the flag");
}

#[test]
fn export_help_describes_optional_context_snapshots() {
    let help = Cli::command().render_long_help().to_string();
    assert!(
        help.contains(
            "Generate optional context snapshots for sharing, offline review, or non-MCP agents"
        ),
        "export help must describe optional context snapshots, got: {help}"
    );
}

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
    assert!(daemon, "--daemon flag must be set");
    assert!(!no_ui, "--no-ui must stay false when not passed");
    assert!(
        command.is_none(),
        "bare watch must not consume a subcommand"
    );

    let no_ui = parse(&["watch", "--no-ui"]);
    let Some(Command::Watch {
        daemon,
        no_ui,
        command,
    }) = no_ui.command
    else {
        panic!("watch --no-ui should parse");
    };
    assert!(!daemon, "--daemon must stay false when not passed");
    assert!(no_ui, "--no-ui flag must be set");
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

#[test]
fn sync_dispatches_to_sync_variant() {
    let cli = parse(&["sync"]);
    matches!(cli.command, Some(Command::Sync { .. }))
        .then_some(())
        .expect("sync should parse to Command::Sync");
}

#[test]
fn check_dispatches_to_check_variant() {
    let cli = parse(&["check"]);
    matches!(cli.command, Some(Command::Check { .. }))
        .then_some(())
        .expect("check should parse to Command::Check");
}

#[test]
fn export_dispatches_to_export_variant() {
    let cli = parse(&["export"]);
    matches!(cli.command, Some(Command::Export { .. }))
        .then_some(())
        .expect("export should parse to Command::Export");
}

#[test]
fn ci_run_dispatches_to_ci_run_variant() {
    let cli = parse(&["ci-run", "--target", "src/lib.rs", "--json"]);
    let Some(Command::CiRun(args)) = cli.command else {
        panic!("ci-run should parse to Command::CiRun");
    };
    assert_eq!(args.targets, vec!["src/lib.rs"]);
    assert!(args.json);
}

#[test]
fn upgrade_dispatches_to_upgrade_variant() {
    let cli = parse(&["upgrade"]);
    matches!(cli.command, Some(Command::Upgrade { .. }))
        .then_some(())
        .expect("upgrade should parse to Command::Upgrade");
}

#[test]
fn agent_setup_dispatches_to_agent_setup_variant() {
    let cli = parse(&["agent-setup", "claude"]);
    matches!(cli.command, Some(Command::AgentSetup(_)))
        .then_some(())
        .expect("agent-setup claude should parse to Command::AgentSetup");
}

#[test]
fn uninstall_dispatches_to_uninstall_variant() {
    let cli = parse(&["uninstall", "--apply", "--force", "--delete-data"]);
    let Some(Command::Uninstall(args)) = cli.command else {
        panic!("uninstall should parse to Command::Uninstall");
    };
    assert!(args.apply);
    assert!(args.force);
    assert!(args.delete_data);
}

#[test]
fn setup_without_tool_parses_to_wizard_mode() {
    let cli = parse(&["setup"]);
    let Some(Command::Setup(args)) = cli.command else {
        panic!("`setup` (no tool) should parse to Command::Setup");
    };
    assert!(
        args.tool.is_none(),
        "omitting the tool positional must leave tool unset so the dispatcher routes to the wizard"
    );
}

#[test]
fn setup_with_tool_still_parses_with_tool_set() {
    let cli = parse(&["setup", "claude"]);
    let Some(Command::Setup(args)) = cli.command else {
        panic!("`setup claude` should parse to Command::Setup");
    };
    assert!(
        args.tool.is_some(),
        "passing a tool positional must populate Command::Setup.tool so the scripted path runs"
    );
}

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
fn context_aliases_parse_numeric_budget() {
    let cards = parse(&["cards", "--query", "where is auth", "--budget", "1500"]);
    let Some(Command::Cards { query, budget }) = cards.command else {
        panic!("cards should parse");
    };
    assert_eq!(query, "where is auth");
    assert_eq!(budget, Some(1500));

    let explain = parse(&["explain", "src/lib.rs", "--budget", "1000"]);
    assert!(
        matches!(
            explain.command,
            Some(Command::Explain {
                budget: Some(1000),
                ..
            })
        ),
        "explain should parse numeric budget"
    );

    let impact = parse(&["impact", "src/lib.rs", "--budget", "2000"]);
    assert!(
        matches!(
            impact.command,
            Some(Command::Impact {
                budget: Some(2000),
                ..
            })
        ),
        "impact should parse numeric budget"
    );

    let tests = parse(&["tests", "src/lib.rs", "--budget", "1200"]);
    assert!(
        matches!(
            tests.command,
            Some(Command::Tests {
                budget: Some(1200),
                ..
            })
        ),
        "tests should parse numeric budget"
    );

    let risks = parse(&["risks", "src/lib.rs", "--budget", "1200"]);
    assert!(
        matches!(
            risks.command,
            Some(Command::Risks {
                budget: Some(1200),
                ..
            })
        ),
        "risks should parse numeric budget"
    );
}

#[test]
fn stats_and_bench_context_parse() {
    let stats = parse(&["stats", "context", "--json"]);
    assert!(
        matches!(
            stats.command,
            Some(Command::Stats(StatsCommand::Context { json: true, .. }))
        ),
        "stats context --json should parse"
    );

    let bench = parse(&[
        "bench",
        "context",
        "--tasks",
        "benches/tasks/*.json",
        "--json",
    ]);
    assert!(
        matches!(
            bench.command,
            Some(Command::Bench(BenchCommand::Context { json: true, .. }))
        ),
        "bench context --json should parse"
    );
}

#[test]
fn dashboard_dispatches_to_dashboard_variant() {
    let cli = parse(&["dashboard"]);
    assert!(
        matches!(cli.command, Some(Command::Dashboard)),
        "dashboard should parse to Command::Dashboard"
    );
}

#[test]
fn repo_flag_is_global_and_survives_on_every_subcommand() {
    // Spot-check a couple of subcommands; the flag is declared `global = true`
    // on `Cli`, so clap propagates it regardless of the subcommand that
    // follows. Asserting on two representative subcommands pins that
    // invariant without exploding into N × M tests.
    let status = parse(&["--repo", "/tmp/x", "status"]);
    assert_eq!(
        status.repo.as_deref(),
        Some(std::path::Path::new("/tmp/x")),
        "--repo must propagate to status"
    );
    let watch = parse(&["--repo", "/tmp/y", "watch", "--daemon"]);
    assert_eq!(
        watch.repo.as_deref(),
        Some(std::path::Path::new("/tmp/y")),
        "--repo must propagate to watch"
    );
}

#[test]
fn no_color_flag_is_global_across_subcommands() {
    let bare = parse(&["--no-color"]);
    assert!(bare.no_color, "--no-color should set on bare synrepo");
    let dashboard = parse(&["--no-color", "dashboard"]);
    assert!(dashboard.no_color, "--no-color should survive on dashboard");
}
