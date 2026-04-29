//! synrepo CLI entry point.
//!
//! Bare `synrepo` (no subcommand) runs a read-only runtime probe and routes
//! the user to the dashboard, guided setup, or guided repair wizard based on
//! classification. All explicit subcommands (`init`, `status`, `watch`,
//! `sync`, `export`, `mcp`, and friends) behave exactly as before.

mod cli_support;

use std::path::Path;

use clap::Parser;
use synrepo::tui::{stdout_is_tty, TuiOptions};
use syntext::SearchOptions;
use tracing_subscriber::EnvFilter;

use cli_support::cli_args::{
    AgentSetupArgs, BenchCommand, Cli, Command, GraphCommand, LinksCommand, NotesCommand,
    ProjectCommand, SetupArgs, StatsCommand, UninstallArgs, WatchCommand,
};
#[cfg(test)]
use cli_support::commands::prepare_mcp_state;
#[cfg(test)]
use cli_support::commands::report_reconcile_outcome;
use cli_support::commands::{
    agent_setup_many_resolved, bench_context, cards_alias, change_risk, check, compact, docs,
    doctor, explain_alias, export, findings, graph_query, graph_stats, handoffs, impact_alias,
    init, links_accept, links_list, links_reject, links_review, node, notes_add, notes_audit,
    notes_forget, notes_link, notes_list, notes_supersede, notes_verify, project_add,
    project_inspect, project_list, project_remove, project_rename, project_use, reconcile, remove,
    resolve_tool_resolution, risks_alias, run_mcp_server, search, server, setup_many_resolved,
    stats_context, status, sync, tests_alias, uninstall, upgrade, watch, watch_internal,
    watch_status, watch_stop, StatFormat,
};
// Re-exported for `cli_support::tests::agent_setup` via `crate::agent_setup`.
// cli.rs dispatches through `agent_setup_many` but the test binary compiles
// without `cfg(test)`, so this import must be unconditional.
#[allow(unused_imports)]
use cli_support::commands::agent_setup;
use cli_support::entry::{run_bare_entrypoint, run_dashboard_command};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let explicit_repo = cli.repo.is_some();
    let repo_root = match cli.repo {
        Some(p) => p,
        None => std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("cannot determine working directory: {e}"))?,
    };

    let tui_opts = TuiOptions {
        no_color: cli.no_color,
    };

    match cli.command {
        None => run_bare_entrypoint(&repo_root, tui_opts, explicit_repo),
        Some(cmd) => dispatch(cmd, &repo_root, tui_opts, explicit_repo),
    }
}

/// Dispatch an explicit subcommand. Behavior for each branch is unchanged
/// from prior releases.
fn dispatch(
    command: Command,
    repo_root: &Path,
    tui_opts: TuiOptions,
    explicit_repo: bool,
) -> anyhow::Result<()> {
    match command {
        Command::Init { mode, gitignore } => init(repo_root, mode.map(Into::into), gitignore),
        Command::Status { json, recent, full } => status(repo_root, json, recent, full),
        Command::Project(ProjectCommand::Add { path }) => project_add(repo_root, path),
        Command::Project(ProjectCommand::List { json }) => project_list(json),
        Command::Project(ProjectCommand::Inspect { path, json }) => {
            project_inspect(repo_root, path, json)
        }
        Command::Project(ProjectCommand::Remove { path }) => project_remove(repo_root, path),
        Command::Project(ProjectCommand::Use { selector }) => project_use(&selector),
        Command::Project(ProjectCommand::Rename { selector, name }) => {
            project_rename(&selector, &name)
        }
        Command::AgentSetup(AgentSetupArgs {
            tool,
            only,
            skip,
            force,
            regen,
        }) => {
            let resolution = resolve_tool_resolution(tool, &only, &skip)?;
            agent_setup_many_resolved(repo_root, &resolution, force, regen)
        }
        Command::Setup(SetupArgs {
            tool,
            only,
            skip,
            force,
            explain,
            gitignore,
            project,
            global,
        }) => {
            if global {
                eprintln!("warning: `synrepo setup --global` is deprecated; global setup is now the default");
            }
            let any_target = tool.is_some() || !only.is_empty() || !skip.is_empty();
            if any_target {
                let resolution = resolve_tool_resolution(tool, &only, &skip)?;
                setup_many_resolved(repo_root, &resolution, force, gitignore, project)?;
                if explain {
                    cli_support::setup_cmd::run_explain_step(repo_root, tui_opts)?;
                }
                return Ok(());
            }
            // Wizard mode owns its own init/explain/gitignore handling via
            // SetupPlan, so the scripted-only flags have no clean place to
            // land. Fail loud rather than silently dropping.
            let mut bad_flags = Vec::new();
            if force {
                bad_flags.push("--force");
            }
            if explain {
                bad_flags.push("--explain");
            }
            if gitignore {
                bad_flags.push("--gitignore");
            }
            if project {
                bad_flags.push("--project");
            }
            if global {
                bad_flags.push("--global");
            }
            if !bad_flags.is_empty() {
                anyhow::bail!(
                    "`synrepo setup` without a tool launches the interactive wizard; \
                     {} only applies when a tool is passed (e.g. `synrepo setup claude {}`).",
                    bad_flags.join(" / "),
                    bad_flags.join(" "),
                );
            }
            if !stdout_is_tty() {
                eprintln!(
                    "synrepo setup: interactive wizard requires a TTY. \
                     Pass a tool for the scripted flow (e.g. `synrepo setup claude`)."
                );
                std::process::exit(2);
            }
            cli_support::setup_cmd::run_wizard_and_apply(repo_root, tui_opts)
        }
        Command::Reconcile { fast } => reconcile(repo_root, fast),
        Command::InstallHooks => cli_support::commands::install_hooks(repo_root),
        Command::Check { json } => check(repo_root, json),
        Command::Sync {
            json,
            generate_cross_links,
            regenerate_cross_links,
            reset_explain_totals,
        } => sync(
            repo_root,
            json,
            generate_cross_links,
            regenerate_cross_links,
            reset_explain_totals,
        ),
        Command::Search {
            query,
            ignore_case,
            file_type,
            exclude_type,
            path_filter,
            max_results,
        } => search(
            repo_root,
            &query,
            SearchOptions {
                path_filter,
                file_type,
                exclude_type,
                max_results,
                case_insensitive: ignore_case,
            },
        ),
        Command::Cards { query, budget } => cards_alias(repo_root, &query, budget),
        Command::Docs(command) => docs(repo_root, command),
        Command::Explain { target, budget } => explain_alias(repo_root, &target, budget),
        Command::Impact { target, budget } => impact_alias(repo_root, &target, budget),
        Command::Tests { target, budget } => tests_alias(repo_root, &target, budget),
        Command::Risks { target, budget } => risks_alias(repo_root, &target, budget),
        Command::Stats(StatsCommand::Context { format, json }) => {
            stats_context(repo_root, StatFormat::from_cli(format, json))
        }
        Command::Bench(BenchCommand::Context { tasks, json }) => {
            bench_context(repo_root, &tasks, json)
        }
        Command::Graph(GraphCommand::Query { q }) => graph_query(repo_root, &q),
        Command::Graph(GraphCommand::Stats) => graph_stats(repo_root),
        Command::Node { id } => node(repo_root, &id),
        Command::Watch {
            daemon,
            no_ui,
            command,
        } => {
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
                    WatchCommand::Status => watch_status(repo_root),
                    WatchCommand::Stop => watch_stop(repo_root),
                }
            } else if daemon {
                watch(repo_root, true)
            } else if no_ui || !stdout_is_tty() {
                // Explicit opt-out OR non-TTY stdout: plain log lines. Mirrors
                // pre-dashboard foreground-watch behavior so piped invocations
                // like `synrepo watch > watch.log` keep working.
                watch(repo_root, false)
            } else {
                // Foreground + TTY + no opt-out = dashboard live mode.
                match synrepo::tui::run_live_watch_dashboard(repo_root, tui_opts) {
                    Ok(_) => Ok(()),
                    Err(err) => {
                        eprintln!(
                            "live dashboard unavailable: {err}; falling back to plain foreground watch \
                             (use `--no-ui` to suppress this notice)"
                        );
                        watch(repo_root, false)
                    }
                }
            }
        }
        Command::Links(LinksCommand::List { tier, limit, json }) => {
            links_list(repo_root, tier.as_deref(), limit, json)
        }
        Command::Links(LinksCommand::Review { limit, json }) => {
            links_review(repo_root, limit, json)
        }
        Command::Links(LinksCommand::Accept {
            candidate_id,
            reviewer,
        }) => links_accept(repo_root, &candidate_id, reviewer.as_deref()),
        Command::Links(LinksCommand::Reject {
            candidate_id,
            reviewer,
        }) => links_reject(repo_root, &candidate_id, reviewer.as_deref()),
        Command::Notes(NotesCommand::Add {
            target_kind,
            target,
            claim,
            created_by,
            confidence,
            evidence_json,
            source_hashes_json,
            graph_revision,
            json,
        }) => notes_add(
            repo_root,
            &target_kind,
            &target,
            &claim,
            &created_by,
            &confidence,
            evidence_json.as_deref(),
            source_hashes_json.as_deref(),
            graph_revision,
            json,
        ),
        Command::Notes(NotesCommand::List {
            target_kind,
            target,
            limit,
            include_all,
            json,
        }) => notes_list(
            repo_root,
            target_kind.as_deref(),
            target.as_deref(),
            limit,
            include_all,
            json,
        ),
        Command::Notes(NotesCommand::Audit {
            target_kind,
            target,
            limit,
            json,
        }) => notes_audit(
            repo_root,
            target_kind.as_deref(),
            target.as_deref(),
            limit,
            json,
        ),
        Command::Notes(NotesCommand::Link {
            from_note,
            to_note,
            actor,
            json,
        }) => notes_link(repo_root, &from_note, &to_note, &actor, json),
        Command::Notes(NotesCommand::Supersede {
            old_note,
            target_kind,
            target,
            claim,
            created_by,
            confidence,
            evidence_json,
            source_hashes_json,
            graph_revision,
            json,
        }) => notes_supersede(
            repo_root,
            &old_note,
            &target_kind,
            &target,
            &claim,
            &created_by,
            &confidence,
            evidence_json.as_deref(),
            source_hashes_json.as_deref(),
            graph_revision,
            json,
        ),
        Command::Notes(NotesCommand::Forget {
            note_id,
            actor,
            reason,
            json,
        }) => notes_forget(repo_root, &note_id, &actor, reason.as_deref(), json),
        Command::Notes(NotesCommand::Verify {
            note_id,
            actor,
            graph_revision,
            json,
        }) => notes_verify(repo_root, &note_id, &actor, graph_revision, json),
        Command::Upgrade { apply } => upgrade(repo_root, apply),
        Command::Compact { apply, policy } => compact(repo_root, apply, policy.into()),
        Command::Export {
            format,
            deep,
            commit,
            out,
        } => export(repo_root, format.into(), deep, commit, out),
        Command::Findings {
            node,
            kind,
            freshness,
            limit,
            json,
        } => findings(
            repo_root,
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
        } => change_risk(repo_root, &target, budget.as_deref(), json),
        Command::CiRun(args) => cli_support::commands::ci_run(repo_root, args),
        Command::Handoffs { limit, since, json } => handoffs(repo_root, limit, since, json),
        Command::WatchInternal => watch_internal(repo_root),
        Command::Doctor { json } => doctor(repo_root, json),
        Command::Dashboard => run_dashboard_command(repo_root, tui_opts),
        Command::Server { metrics } => server(repo_root, &metrics),
        Command::Mcp { allow_edits } => run_mcp_server(repo_root, allow_edits, explicit_repo),
        Command::Remove {
            tool,
            apply,
            json,
            keep_synrepo_dir,
            force,
        } => remove(repo_root, tool, apply, json, keep_synrepo_dir, force),
        Command::Uninstall(UninstallArgs {
            apply,
            json,
            force,
            delete_data,
            keep_binary,
        }) => uninstall(repo_root, apply, json, force, delete_data, keep_binary),
    }
}
