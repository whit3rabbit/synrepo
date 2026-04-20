//! synrepo CLI entry point.
//!
//! Bare `synrepo` (no subcommand) runs a read-only runtime probe and routes
//! the user to the dashboard, guided setup, or guided repair wizard based on
//! classification. All explicit subcommands (`init`, `status`, `watch`,
//! `sync`, `export`, `mcp`, and friends) behave exactly as before.
//!
//! All non-trivial logic lives in the library crate or local support modules.

mod cli_support;

use std::path::Path;

use clap::Parser;
use synrepo::bootstrap::runtime_probe::{probe, Missing, RoutingDecision};
use synrepo::tui::{
    run_dashboard, run_integration_wizard, run_live_watch_dashboard, run_repair_wizard,
    run_setup_wizard, run_synthesis_only_wizard, stdout_is_tty, DashboardOptions, IntegrationPlan,
    IntegrationWizardOutcome, RepairPlan, RepairWizardOutcome, SetupPlan, SetupWizardOutcome,
    TuiOptions, TuiOutcome,
};
use syntext::SearchOptions;
use tracing_subscriber::EnvFilter;

use cli_support::agent_shims::{registry as shim_registry, AgentTool, AutomationTier};
use cli_support::cli_args::{Cli, Command, GraphCommand, LinksCommand, WatchCommand};
#[cfg(test)]
use cli_support::commands::prepare_mcp_state;
#[cfg(test)]
use cli_support::commands::report_reconcile_outcome;
use cli_support::commands::{
    agent_setup, change_risk, check, compact, export, findings, graph_query, graph_stats, handoffs,
    init, links_accept, links_list, links_reject, links_review, node, reconcile, remove,
    run_mcp_server, search, setup, status, status_output, step_apply_integration,
    step_apply_synthesis, step_backup_mcp_config, step_ensure_ready, step_init, step_register_mcp,
    step_write_shim, sync, upgrade, watch, watch_internal, watch_status, watch_stop,
};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let repo_root = match cli.repo {
        Some(p) => p,
        None => std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("cannot determine working directory: {e}"))?,
    };

    let tui_opts = TuiOptions {
        no_color: cli.no_color,
    };

    match cli.command {
        None => run_bare_entrypoint(&repo_root, tui_opts),
        Some(cmd) => dispatch(cmd, &repo_root, tui_opts),
    }
}

/// Bare `synrepo`: probe, route, and run the appropriate TUI entrypoint.
fn run_bare_entrypoint(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<()> {
    let report = probe(repo_root);
    let decision = RoutingDecision::from_report(&report);
    let is_tty = stdout_is_tty();

    match decision {
        RoutingDecision::OpenDashboard { integration } => {
            if !is_tty {
                print!("{}", bare_ready_summary(repo_root)?);
                return Ok(());
            }
            run_dashboard_with_sub_wizards(repo_root, integration, DashboardOptions::from(opts))
        }
        RoutingDecision::OpenSetup => {
            if !is_tty {
                eprint!("{}", bare_uninitialized_fallback());
                std::process::exit(2);
            }
            match run_setup_wizard(repo_root, opts)? {
                SetupWizardOutcome::Completed { plan } => {
                    execute_setup_plan(repo_root, plan)?;
                    open_dashboard_after_wizard(repo_root, opts)
                }
                SetupWizardOutcome::Cancelled => {
                    println!("setup wizard cancelled; no changes applied.");
                    Ok(())
                }
                SetupWizardOutcome::NonTty => {
                    eprint!("{}", bare_uninitialized_fallback());
                    std::process::exit(2);
                }
            }
        }
        RoutingDecision::OpenRepair { missing } => {
            if !is_tty {
                eprint!("{}", bare_partial_fallback(&missing));
                std::process::exit(2);
            }
            match run_repair_wizard(repo_root, missing, opts)? {
                RepairWizardOutcome::Completed { plan } => execute_repair_plan(repo_root, plan),
                RepairWizardOutcome::Cancelled => {
                    println!("repair wizard cancelled; no changes applied.");
                    Ok(())
                }
                RepairWizardOutcome::NonTty => {
                    eprint!("{}", bare_partial_fallback(&[]));
                    std::process::exit(2);
                }
            }
        }
    }
}

/// Non-TTY plain-text summary printed when bare `synrepo` runs on a ready
/// repo behind a pipe or redirect. Mirrors the key lines from `synrepo status`.
fn bare_ready_summary(repo_root: &Path) -> anyhow::Result<String> {
    status_output(repo_root, false, false, false)
}

/// Run the poll-mode dashboard in a loop so dashboard-launched sub-wizards can
/// tear down the alt-screen, execute their plan, and re-open the dashboard
/// with a fresh probe and integration signal. Returns once the operator quits
/// the dashboard normally or a non-TTY fallback fires.
fn run_dashboard_with_sub_wizards(
    repo_root: &Path,
    mut integration: synrepo::bootstrap::runtime_probe::AgentIntegration,
    mut opts: DashboardOptions,
) -> anyhow::Result<()> {
    loop {
        // `run_dashboard` only returns Exited, NonTtyFallback, or
        // LaunchIntegrationRequested — never a Wizard* variant. An
        // exhaustive match flags future TuiOutcome additions at compile time.
        match run_dashboard(repo_root, integration.clone(), opts)? {
            TuiOutcome::Exited | TuiOutcome::NonTtyFallback => return Ok(()),
            TuiOutcome::LaunchIntegrationRequested => {
                // Tear-down of the alt-screen has already happened inside
                // `run_dashboard`; safe to print and prompt now.
                let tui_opts = TuiOptions {
                    no_color: opts.no_color,
                };
                match run_integration_wizard(repo_root, integration.clone(), tui_opts)? {
                    IntegrationWizardOutcome::Completed { plan } => {
                        execute_integration_plan(repo_root, plan)?;
                    }
                    IntegrationWizardOutcome::Cancelled => {
                        println!("integration wizard cancelled; no changes applied.");
                    }
                    IntegrationWizardOutcome::NonTty => return Ok(()),
                }
                // Re-probe so the dashboard reflects the new integration
                // state on re-open. Suppress the welcome banner on re-open —
                // the banner is a first-run-only affordance.
                let report = probe(repo_root);
                integration = report.agent_integration;
                opts.welcome_banner = false;
            }
            outcome @ (TuiOutcome::WizardCompleted | TuiOutcome::WizardCancelled) => {
                debug_assert!(
                    false,
                    "run_dashboard returned unexpected outcome: {outcome:?}"
                );
                return Ok(());
            }
        }
    }
}

/// Execute a completed [`IntegrationPlan`] after the TUI alt-screen has been
/// torn down. Splits the plan so the wizard can request shim-only, MCP-only,
/// or both — `step_apply_integration` would force both.
fn execute_integration_plan(repo_root: &Path, plan: IntegrationPlan) -> anyhow::Result<()> {
    let tool = AgentTool::from_target_kind(plan.target);
    if plan.write_shim {
        step_write_shim(repo_root, tool, plan.overwrite_shim)?;
    }
    let mut backup: Option<String> = None;
    if plan.register_mcp {
        backup = step_backup_mcp_config(repo_root, tool)?;
        step_register_mcp(repo_root, tool)?;
    }
    let wrote_mcp =
        plan.register_mcp && matches!(tool.automation_tier(), AutomationTier::Automated);
    shim_registry::record_install_best_effort(repo_root, tool, wrote_mcp, backup);
    println!("Integration complete.");
    Ok(())
}

/// Execute a completed [`SetupPlan`] after the TUI alt-screen has been torn
/// down. All file-system writes happen here, not inside the library.
fn execute_setup_plan(repo_root: &Path, plan: SetupPlan) -> anyhow::Result<()> {
    println!("synrepo setup: applying plan.");
    step_init(repo_root, Some(plan.mode), false, false)?;
    if let Some(target) = plan.target {
        let tool = AgentTool::from_target_kind(target);
        let backup = step_backup_mcp_config(repo_root, tool)?;
        step_apply_integration(repo_root, tool, false)?;
        let wrote_mcp = matches!(tool.automation_tier(), AutomationTier::Automated);
        shim_registry::record_install_best_effort(repo_root, tool, wrote_mcp, backup);
    }
    if plan.synthesis.is_some() {
        step_apply_synthesis(repo_root, plan.synthesis.as_ref())?;
    }
    if plan.reconcile_after {
        // Setup promises an operationally ready repo, not just a populated
        // graph. The shared helper runs the first reconcile only when the
        // reconcile-state file is still missing.
        step_ensure_ready(repo_root)?;
    }
    println!("Setup complete. Repo is ready.");
    Ok(())
}

/// Launch the synthesis-only sub-wizard after `synrepo setup <tool> --synthesis`,
/// patching `.synrepo/config.toml` with the chosen `[synthesis]` block. Non-TTY
/// callers get a pointer to the config file instead of crashing.
fn run_synthesis_step(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<()> {
    match run_synthesis_only_wizard(opts)? {
        SetupWizardOutcome::Completed { plan } => {
            step_apply_synthesis(repo_root, plan.synthesis.as_ref())?;
            Ok(())
        }
        SetupWizardOutcome::Cancelled => {
            println!("synthesis sub-wizard cancelled; config.toml untouched.");
            Ok(())
        }
        SetupWizardOutcome::NonTty => {
            println!(
                "--synthesis requires a TTY. Edit .synrepo/config.toml manually; see \
                 AGENTS.md for the `[synthesis]` block schema."
            );
            Ok(())
        }
    }
}

/// After a successful setup wizard, re-probe and open the dashboard with the
/// one-shot welcome banner seeded in the log pane. A partial re-classification
/// is unexpected here (setup just ran to completion), but we still fall
/// through gracefully rather than re-entering a wizard.
fn open_dashboard_after_wizard(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<()> {
    if !stdout_is_tty() {
        return Ok(());
    }
    let report = probe(repo_root);
    let decision = RoutingDecision::from_report(&report);
    match decision {
        RoutingDecision::OpenDashboard { integration } => {
            let dashboard_opts = DashboardOptions {
                no_color: opts.no_color,
                welcome_banner: true,
            };
            run_dashboard_with_sub_wizards(repo_root, integration, dashboard_opts)
        }
        _ => {
            // Setup completed but probe still sees the repo as non-ready
            // (unusual — e.g. a compat-advisory left the store in a blocked
            // state). Print a status summary so the operator has something
            // actionable and exit cleanly.
            print!("{}", bare_ready_summary(repo_root).unwrap_or_default());
            Ok(())
        }
    }
}

/// Execute a completed [`RepairPlan`] after the TUI alt-screen has been torn
/// down. Actions run in order: write config, upgrade-apply, reconcile, shim.
/// The probe is re-run between mutating steps so later steps see fresh state
/// and a transient success transitions cleanly to the dashboard on the next
/// bare-`synrepo` run.
fn execute_repair_plan(repo_root: &Path, plan: RepairPlan) -> anyhow::Result<()> {
    if plan.is_empty() {
        println!("synrepo repair: plan empty, nothing to do.");
        return Ok(());
    }
    println!("synrepo repair: applying plan.");
    if plan.write_config {
        println!("  Writing default config.toml...");
        // `step_init` with force=false is idempotent on an existing repo and
        // creates `.synrepo/config.toml` if missing. It is the canonical path
        // for config bootstrap.
        step_init(repo_root, None, false, false)?;
        let _ = probe(repo_root);
    }
    if plan.run_upgrade_apply {
        println!("  Running `synrepo upgrade --apply`...");
        upgrade(repo_root, true)?;
        let _ = probe(repo_root);
    }
    if plan.run_reconcile {
        println!("  Running reconcile pass...");
        reconcile(repo_root)?;
        let _ = probe(repo_root);
    }
    if let Some(target) = plan.write_shim_for {
        let tool = AgentTool::from_target_kind(target);
        println!(
            "  Writing {} {}...",
            tool.display_name(),
            tool.artifact_label()
        );
        let backup = step_backup_mcp_config(repo_root, tool)?;
        step_apply_integration(repo_root, tool, false)?;
        let wrote_mcp = matches!(tool.automation_tier(), AutomationTier::Automated);
        shim_registry::record_install_best_effort(repo_root, tool, wrote_mcp, backup);
    }
    println!("Repair complete.");
    Ok(())
}

fn bare_uninitialized_fallback() -> String {
    "\
synrepo: this repository is not initialized.
Run `synrepo init` to create .synrepo/ and populate the graph.
"
    .to_string()
}

fn bare_partial_fallback(missing: &[Missing]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    writeln!(
        out,
        "synrepo: this repository has a partial .synrepo/ install."
    )
    .unwrap();
    if !missing.is_empty() {
        writeln!(out, "Missing or blocked components:").unwrap();
        for m in missing {
            writeln!(out, "  - {}", missing_label(m)).unwrap();
        }
    }
    writeln!(
        out,
        "Run `synrepo status` for detail or `synrepo upgrade` for compat actions."
    )
    .unwrap();
    out
}

fn missing_label(m: &Missing) -> String {
    match m {
        Missing::ConfigFile => ".synrepo/config.toml missing".to_string(),
        Missing::ConfigUnreadable { detail } => format!("config.toml unreadable: {detail}"),
        Missing::GraphStore => ".synrepo/graph/ missing or empty".to_string(),
        Missing::CompatBlocked { guidance } => {
            if let Some(first) = guidance.first() {
                format!("store compat action required: {first}")
            } else {
                "store compat action required".to_string()
            }
        }
        Missing::CompatEvaluationFailed { detail } => format!("compat evaluation failed: {detail}"),
    }
}

/// Explicit `synrepo dashboard`: probe, but exit non-zero on non-ready state
/// instead of routing to a wizard. Keeps scripted invocations deterministic.
fn run_dashboard_command(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<()> {
    let report = probe(repo_root);
    let decision = RoutingDecision::from_report(&report);
    match decision {
        RoutingDecision::OpenDashboard { integration } => {
            if !stdout_is_tty() {
                print!("{}", bare_ready_summary(repo_root)?);
                return Ok(());
            }
            run_dashboard_with_sub_wizards(repo_root, integration, DashboardOptions::from(opts))
        }
        RoutingDecision::OpenSetup => {
            eprintln!(
                "synrepo dashboard: repository is uninitialized. Run `synrepo` (bare) or `synrepo init` to set up."
            );
            std::process::exit(2);
        }
        RoutingDecision::OpenRepair { missing } => {
            eprintln!(
                "synrepo dashboard: repository has a partial install. Run `synrepo` (bare) to open the repair wizard, or `synrepo status` to inspect."
            );
            for m in &missing {
                eprintln!("  - {}", missing_label(m));
            }
            std::process::exit(2);
        }
    }
}

/// Dispatch an explicit subcommand. Behavior for each branch is unchanged
/// from prior releases.
fn dispatch(command: Command, repo_root: &Path, tui_opts: TuiOptions) -> anyhow::Result<()> {
    match command {
        Command::Init { mode, gitignore } => init(repo_root, mode.map(Into::into), gitignore),
        Command::Status { json, recent, full } => status(repo_root, json, recent, full),
        Command::AgentSetup { tool, force, regen } => agent_setup(repo_root, tool, force, regen),
        Command::Setup {
            tool,
            force,
            synthesis,
            gitignore,
        } => {
            setup(repo_root, tool, force, gitignore)?;
            if synthesis {
                run_synthesis_step(repo_root, tui_opts)?;
            }
            Ok(())
        }
        Command::Reconcile => reconcile(repo_root),
        Command::Check { json } => check(repo_root, json),
        Command::Sync {
            json,
            generate_cross_links,
            regenerate_cross_links,
        } => sync(
            repo_root,
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
                match run_live_watch_dashboard(repo_root, tui_opts) {
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
        Command::Handoffs { limit, since, json } => handoffs(repo_root, limit, since, json),
        Command::WatchInternal => watch_internal(repo_root),
        Command::Dashboard => run_dashboard_command(repo_root, tui_opts),
        Command::Mcp => run_mcp_server(repo_root),
        Command::Remove {
            tool,
            apply,
            json,
            keep_synrepo_dir,
            force,
        } => remove(repo_root, tool, apply, json, keep_synrepo_dir, force),
    }
}
