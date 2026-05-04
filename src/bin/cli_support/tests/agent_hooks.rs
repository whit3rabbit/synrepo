use clap::Parser;

use super::super::cli_args::{AgentHookCommand, Cli, Command};

fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["synrepo"];
    full.extend_from_slice(args);
    Cli::try_parse_from(full).expect("args should parse")
}

#[test]
fn setup_with_agent_hooks_flag_round_trips() {
    let cli = parse(&["setup", "codex", "--agent-hooks"]);
    let Some(Command::Setup(args)) = cli.command else {
        panic!("`setup codex --agent-hooks` should parse to Command::Setup");
    };
    assert!(args.tool.is_some());
    assert!(args.agent_hooks);
}

#[test]
fn hidden_agent_hook_nudge_dispatches() {
    let cli = parse(&[
        "agent-hook",
        "nudge",
        "--client",
        "codex",
        "--event",
        "UserPromptSubmit",
    ]);
    let Some(Command::AgentHook(AgentHookCommand::Nudge(args))) = cli.command else {
        panic!("agent-hook nudge should parse");
    };
    assert_eq!(args.client, "codex");
    assert_eq!(args.event, "UserPromptSubmit");
}
