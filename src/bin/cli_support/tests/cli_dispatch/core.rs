use clap::CommandFactory;
use tempfile::tempdir;

use super::super::super::cli_args::Command;
use super::super::super::entry::bare_ready_summary;
use super::parse;

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
    let help = super::super::super::cli_args::Cli::command()
        .render_long_help()
        .to_string();
    assert!(
        help.contains(
            "Generate optional context snapshots for sharing, offline review, or non-MCP agents"
        ),
        "export help must describe optional context snapshots, got: {help}"
    );
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
fn resume_context_dispatches_to_resume_context_variant() {
    let cli = parse(&[
        "resume-context",
        "--limit",
        "2",
        "--since-days",
        "7",
        "--budget-tokens",
        "500",
        "--no-notes",
        "--json",
    ]);
    let Some(Command::ResumeContext {
        limit,
        since_days,
        budget_tokens,
        no_notes,
        json,
    }) = cli.command
    else {
        panic!("resume-context should parse to Command::ResumeContext");
    };
    assert_eq!(limit, Some(2));
    assert_eq!(since_days, Some(7));
    assert_eq!(budget_tokens, Some(500));
    assert!(no_notes);
    assert!(json);
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
    assert!(args.tool.is_none());
}

#[test]
fn setup_with_tool_still_parses_with_tool_set() {
    let cli = parse(&["setup", "claude"]);
    let Some(Command::Setup(args)) = cli.command else {
        panic!("`setup claude` should parse to Command::Setup");
    };
    assert!(args.tool.is_some());
}

#[test]
fn dashboard_dispatches_to_dashboard_variant() {
    let cli = parse(&["dashboard"]);
    assert!(
        matches!(cli.command, Some(Command::Dashboard)),
        "dashboard should parse to Command::Dashboard"
    );
}
