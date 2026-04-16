//! synrepo CLI entry point.
//!
//! Phase 0/1 subcommands:
//! - `synrepo mcp`, start MCP server over stdio
//! - `synrepo init [--mode auto|curated]`, create `.synrepo/` in the current repo
//! - `synrepo status`, print operational health: mode, graph counts, last reconcile, lock state
//! - `synrepo agent-setup <tool>`, generate a thin integration shim for claude/cursor/copilot/generic
//! - `synrepo reconcile`, run a structural compile pass without full re-bootstrap
//! - `synrepo check`, read-only drift report across all repair surfaces
//! - `synrepo sync`, repair auto-fixable drift surfaces and log the outcome
//! - `synrepo watch [--daemon]`, keep `.synrepo/` fresh for the current repo
//! - `synrepo search <query>`, lexical search against the persisted index
//! - `synrepo graph query "<direction> <node_id> [edge_kind]"`, narrow graph traversal query
//! - `synrepo node <id>`, dump a node's metadata
//!
//! All non-trivial logic lives in the library crate or local support modules.

mod cli_support;

use clap::Parser;
use syntext::SearchOptions;
use tracing_subscriber::EnvFilter;

use cli_support::cli_args::{Cli, Command, GraphCommand, LinksCommand, WatchCommand};
#[cfg(test)]
use cli_support::commands::report_reconcile_outcome;
use cli_support::commands::{
    agent_setup, change_risk, check, compact, export, findings, graph_query, graph_stats, init,
    links_accept, links_list, links_reject, links_review, node, reconcile, run_mcp_server, search,
    status, sync, upgrade, watch, watch_internal, watch_status, watch_stop,
};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let repo_root = match cli.repo {
        Some(p) => p,
        None => std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("cannot determine working directory: {e}"))?,
    };

    match cli.command {
        Command::Init { mode } => init(&repo_root, mode.map(Into::into)),
        Command::Status { json, recent } => status(&repo_root, json, recent),
        Command::AgentSetup { tool, force, regen } => agent_setup(&repo_root, tool, force, regen),
        Command::Reconcile => reconcile(&repo_root),
        Command::Check { json } => check(&repo_root, json),
        Command::Sync {
            json,
            generate_cross_links,
            regenerate_cross_links,
        } => sync(
            &repo_root,
            json,
            generate_cross_links,
            regenerate_cross_links,
        ),
        Command::Search {
            query,
            ignore_case,
            file_type,
            exclude_type,
            path_filter,
            max_results,
        } => search(
            &repo_root,
            &query,
            SearchOptions {
                path_filter,
                file_type,
                exclude_type,
                max_results,
                case_insensitive: ignore_case,
            },
        ),
        Command::Graph(GraphCommand::Query { q }) => graph_query(&repo_root, &q),
        Command::Graph(GraphCommand::Stats) => graph_stats(&repo_root),
        Command::Node { id } => node(&repo_root, &id),
        Command::Watch { daemon, command } => {
            if let Some(subcmd) = command {
                if daemon {
                    anyhow::bail!(
                        "`--daemon` has no effect with `watch {}`",
                        match subcmd {
                            WatchCommand::Status => "status",
                            WatchCommand::Stop => "stop",
                        }
                    );
                }
                match subcmd {
                    WatchCommand::Status => watch_status(&repo_root),
                    WatchCommand::Stop => watch_stop(&repo_root),
                }
            } else {
                watch(&repo_root, daemon)
            }
        }
        Command::Links(LinksCommand::List { tier, json }) => {
            links_list(&repo_root, tier.as_deref(), json)
        }
        Command::Links(LinksCommand::Review { limit, json }) => {
            links_review(&repo_root, limit, json)
        }
        Command::Links(LinksCommand::Accept {
            candidate_id,
            reviewer,
        }) => links_accept(&repo_root, &candidate_id, reviewer.as_deref()),
        Command::Links(LinksCommand::Reject {
            candidate_id,
            reviewer,
        }) => links_reject(&repo_root, &candidate_id, reviewer.as_deref()),
        Command::Upgrade { apply } => upgrade(&repo_root, apply),
        Command::Compact { apply, policy } => compact(&repo_root, apply, policy.into()),
        Command::Export {
            format,
            deep,
            commit,
            out,
        } => export(&repo_root, format.into(), deep, commit, out),
        Command::Findings {
            node,
            kind,
            freshness,
            limit,
            json,
        } => findings(
            &repo_root,
            node.as_deref(),
            kind.as_deref(),
            freshness.as_deref(),
            limit,
            json,
        ),
        Command::ChangeRisk {
            target,
            budget,
            json,
        } => change_risk(&repo_root, &target, budget.as_deref(), json),
        Command::WatchInternal => watch_internal(&repo_root),
        Command::Mcp => run_mcp_server(&repo_root),
    }
}
