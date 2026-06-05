use std::path::Path;

use synrepo::tui::{stdout_is_tty, TuiOptions};
use syntext::SearchOptions;

use super::cli_args::{
    AgentHookCommand, AgentSetupArgs, BenchCommand, Command, LinksCommand, NotesCommand,
    ProjectCommand, StatsCommand, UninstallArgs, WatchCommand,
};
use super::commands::{
    agent_setup_many_resolved, ask_alias, bench_context, bench_search, cards_alias, change_risk,
    check, compact, docs, doctor, embeddings, explain_alias, export, findings, graph, handoffs,
    impact_alias, lesson_forget, lesson_list, lesson_recall, lesson_remember, lesson_verify,
    links_accept, links_list, links_reject, links_review, node, notes_add, notes_audit,
    notes_forget, notes_link, notes_list, notes_supersede, notes_verify, orient_alias, project_add,
    project_inspect, project_list, project_prune_missing, project_remove, project_rename,
    project_use, reconcile, remove, resolve_tool_resolution, resume_context, risks_alias,
    run_mcp_server, server, stats_context, status, sync, task_route, tests_alias, uninstall,
    upgrade, watch, watch_internal, watch_status, watch_stop, StatFormat,
};
use super::entry::run_dashboard_command;

pub(crate) fn dispatch(
    command: Command,
    repo_root: &Path,
    tui_opts: TuiOptions,
    explicit_repo: bool,
) -> anyhow::Result<()> {
    match command {
        Command::Init {
            mode,
            gitignore,
            force,
            generate_commentary,
        } => super::setup_cmd::run_init_or_setup(
            repo_root,
            mode.map(Into::into),
            gitignore,
            force,
            generate_commentary,
            tui_opts,
        ),
        Command::Status { json, recent, full } => status(repo_root, json, recent, full),
        Command::Project(ProjectCommand::Add { path }) => project_add(repo_root, path),
        Command::Project(ProjectCommand::List { json }) => project_list(json),
        Command::Project(ProjectCommand::Inspect { path, json }) => {
            project_inspect(repo_root, path, json)
        }
        Command::Project(ProjectCommand::Remove { path }) => project_remove(repo_root, path),
        Command::Project(ProjectCommand::PruneMissing { apply, json }) => {
            project_prune_missing(apply, json)
        }
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
        Command::AgentHook(AgentHookCommand::Nudge(args)) => {
            super::commands::agent_hooks::run_nudge(&args.client, &args.event)
        }
        Command::Setup(args) => super::setup_dispatch::dispatch_setup(repo_root, args, tui_opts),
        Command::Reconcile { fast } => reconcile(repo_root, fast),
        Command::InstallHooks => super::commands::install_hooks(repo_root),
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
            mode,
        } => super::commands::search_with_mode(
            repo_root,
            &query,
            SearchOptions {
                path_filter,
                file_type,
                exclude_type,
                max_results,
                case_insensitive: ignore_case,
            },
            mode,
        ),
        Command::Cards { query, budget } => cards_alias(repo_root, &query, budget),
        Command::Orient => orient_alias(repo_root),
        Command::Ask { ask, budget } => ask_alias(repo_root, &ask, budget),
        Command::TaskRoute { task, path, json } => {
            task_route(repo_root, &task, path.as_deref(), json)
        }
        Command::Docs(command) => docs(repo_root, command),
        Command::Embeddings(command) => embeddings(repo_root, command),
        Command::Explain { target, budget } => explain_alias(repo_root, &target, budget),
        Command::Impact { target, budget } => impact_alias(repo_root, &target, budget),
        Command::Tests { target, budget } => tests_alias(repo_root, &target, budget),
        Command::Risks { target, budget } => risks_alias(repo_root, &target, budget),
        Command::Stats(StatsCommand::Context { format, json }) => {
            stats_context(repo_root, StatFormat::from_cli(format, json))
        }
        Command::Bench(BenchCommand::Context { tasks, mode, json }) => {
            bench_context(repo_root, &tasks, &mode, json)
        }
        Command::Bench(BenchCommand::Search { tasks, mode, json }) => {
            bench_search(repo_root, &tasks, &mode, json)
        }
        Command::Graph(command) => graph(repo_root, command, tui_opts),
        Command::Node { id } => node(repo_root, &id),
        Command::Watch {
            daemon,
            no_ui,
            command,
        } => dispatch_watch(repo_root, tui_opts, daemon, no_ui, command),
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
        Command::Notes(command) => dispatch_notes(repo_root, command),
        Command::Remember(args) => lesson_remember(repo_root, args),
        Command::Recall(args) => lesson_recall(repo_root, args),
        Command::Lessons(args) => lesson_list(repo_root, args),
        Command::Forget(args) => lesson_forget(repo_root, args),
        Command::VerifyLesson(args) => lesson_verify(repo_root, args),
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
        Command::CiRun(args) => super::commands::ci_run(repo_root, args),
        Command::Handoffs { limit, since, json } => handoffs(repo_root, limit, since, json),
        Command::ResumeContext {
            limit,
            since_days,
            budget_tokens,
            no_notes,
            json,
        } => resume_context(repo_root, limit, since_days, budget_tokens, no_notes, json),
        Command::WatchInternal => watch_internal(repo_root),
        Command::Doctor { json } => doctor(repo_root, json),
        Command::Dashboard => run_dashboard_command(repo_root, tui_opts),
        Command::Server { metrics } => server(repo_root, &metrics),
        Command::Mcp {
            allow_overlay_writes,
            allow_source_edits,
            allow_edits,
            call_timeout,
        } => run_mcp_server(
            repo_root,
            allow_overlay_writes,
            allow_source_edits || allow_edits,
            explicit_repo,
            &call_timeout,
        ),
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

fn dispatch_watch(
    repo_root: &Path,
    tui_opts: TuiOptions,
    daemon: bool,
    no_ui: bool,
    command: Option<WatchCommand>,
) -> anyhow::Result<()> {
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
        return match subcmd {
            WatchCommand::Status => watch_status(repo_root),
            WatchCommand::Stop => watch_stop(repo_root),
        };
    }
    if daemon {
        return watch(repo_root, true);
    }
    if no_ui || !stdout_is_tty() {
        return watch(repo_root, false);
    }
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

fn dispatch_notes(repo_root: &Path, command: NotesCommand) -> anyhow::Result<()> {
    match command {
        NotesCommand::Add {
            target_kind,
            target,
            claim,
            created_by,
            confidence,
            evidence_json,
            source_hashes_json,
            graph_revision,
            json,
        } => notes_add(
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
        NotesCommand::List {
            target_kind,
            target,
            limit,
            include_all,
            json,
        } => notes_list(
            repo_root,
            target_kind.as_deref(),
            target.as_deref(),
            limit,
            include_all,
            json,
        ),
        NotesCommand::Audit {
            target_kind,
            target,
            limit,
            json,
        } => notes_audit(
            repo_root,
            target_kind.as_deref(),
            target.as_deref(),
            limit,
            json,
        ),
        NotesCommand::Link {
            from_note,
            to_note,
            actor,
            json,
        } => notes_link(repo_root, &from_note, &to_note, &actor, json),
        NotesCommand::Supersede {
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
        } => notes_supersede(
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
        NotesCommand::Forget {
            note_id,
            actor,
            reason,
            json,
        } => notes_forget(repo_root, &note_id, &actor, reason.as_deref(), json),
        NotesCommand::Verify {
            note_id,
            actor,
            graph_revision,
            json,
        } => notes_verify(repo_root, &note_id, &actor, graph_revision, json),
    }
}
