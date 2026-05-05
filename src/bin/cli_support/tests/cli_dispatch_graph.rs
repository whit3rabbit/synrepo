//! Extra clap dispatch coverage split out to keep `cli_dispatch.rs` small.

use clap::Parser;

use super::super::cli_args::{Cli, Command, GraphCommand, GraphDirectionArg};

fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["synrepo"];
    full.extend_from_slice(args);
    Cli::try_parse_from(full).expect("args should parse")
}

#[test]
fn graph_view_flags_parse() {
    let cli = parse(&[
        "graph",
        "view",
        "src/lib.rs",
        "--direction",
        "outbound",
        "--edge-kind",
        "defines",
        "--edge-kind",
        "calls",
        "--depth",
        "2",
        "--limit",
        "50",
        "--json",
    ]);
    let Some(Command::Graph(GraphCommand::View {
        target,
        direction,
        edge_kind,
        depth,
        limit,
        json,
    })) = cli.command
    else {
        panic!("graph view should parse to Command::Graph(GraphCommand::View)");
    };
    assert_eq!(target.as_deref(), Some("src/lib.rs"));
    assert!(matches!(direction, GraphDirectionArg::Outbound));
    assert_eq!(edge_kind, vec!["defines", "calls"]);
    assert_eq!(depth, 2);
    assert_eq!(limit, 50);
    assert!(json);
}

#[test]
fn mcp_dispatches_to_mcp_variant() {
    let cli = parse(&["mcp"]);
    let Some(Command::Mcp {
        allow_overlay_writes,
        allow_source_edits,
        allow_edits,
        ..
    }) = cli.command
    else {
        panic!("mcp should parse to Command::Mcp");
    };
    assert!(!allow_overlay_writes);
    assert!(!allow_source_edits);
    assert!(!allow_edits);
}

#[test]
fn mcp_mutation_flags_set_explicit_gates() {
    let cli = parse(&["mcp", "--allow-overlay-writes", "--allow-source-edits"]);
    let Some(Command::Mcp {
        allow_overlay_writes,
        allow_source_edits,
        allow_edits,
        ..
    }) = cli.command
    else {
        panic!("mcp mutation flags should parse to Command::Mcp");
    };
    assert!(allow_overlay_writes);
    assert!(allow_source_edits);
    assert!(!allow_edits);
}

#[test]
fn hidden_mcp_allow_edits_alias_still_parses() {
    let cli = parse(&["mcp", "--allow-edits"]);
    let Some(Command::Mcp { allow_edits, .. }) = cli.command else {
        panic!("mcp --allow-edits should parse to Command::Mcp");
    };
    assert!(allow_edits);
}
