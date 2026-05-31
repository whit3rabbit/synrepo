use std::path::{Path, PathBuf};

use synrepo::bootstrap::runtime_probe::{probe, Missing, RoutingDecision};
use synrepo::config::Config;
use synrepo::registry;
use synrepo::tui::{
    run_global_dashboard_with_active_project, run_repair_wizard, stdout_is_tty, DashboardOptions,
    RepairWizardOutcome, TuiOptions, TuiOutcome,
};

use super::repair_cmd::{
    execute_repair_plan, handle_dashboard_outcome, run_dashboard_with_sub_wizards,
    DashboardLoopControl,
};
use super::setup_cmd::run_wizard_and_apply;

/// Bare `synrepo`: probe, route, and run the appropriate TUI entrypoint.
pub(crate) fn run_bare_entrypoint(
    repo_root: &Path,
    opts: TuiOptions,
    explicit_repo: bool,
) -> anyhow::Result<()> {
    let resolved_root = if explicit_repo {
        repo_root.to_path_buf()
    } else if let Some(root) = find_initialized_project_root(repo_root) {
        root
    } else if stdout_is_tty() && !has_git_ancestor(repo_root) && registry_has_projects()? {
        return run_global_dashboard_with_sub_wizards(repo_root, opts);
    } else {
        repo_root.to_path_buf()
    };

    let repo_root = resolved_root.as_path();
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
            run_wizard_and_apply(repo_root, opts)
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

fn run_global_dashboard_with_sub_wizards(cwd: &Path, opts: TuiOptions) -> anyhow::Result<()> {
    let mut dashboard_opts = DashboardOptions::from(opts);
    let mut open_picker = true;
    loop {
        let result = run_global_dashboard_with_active_project(cwd, dashboard_opts, open_picker)?;
        let outcome = result.outcome;
        let action_label = global_dashboard_action_label(&outcome);
        let active_root = match result.active_root {
            Some(root) => root,
            None if matches!(&outcome, TuiOutcome::Exited | TuiOutcome::NonTtyFallback) => {
                return Ok(());
            }
            None => {
                anyhow::bail!("global dashboard requested {action_label} with no active project");
            }
        };
        let mut integration = probe(&active_root).agent_integration;
        match handle_dashboard_outcome(
            &active_root,
            &mut integration,
            &mut dashboard_opts,
            outcome,
        )? {
            DashboardLoopControl::Exit => return Ok(()),
            DashboardLoopControl::SwitchProject(next_root) => {
                let Some(selector) = next_root.to_str() else {
                    anyhow::bail!(
                        "global dashboard cannot switch to non-UTF-8 project path: {}",
                        next_root.display()
                    );
                };
                registry::mark_project_opened(selector)?;
                dashboard_opts.welcome_banner = false;
                open_picker = false;
            }
            DashboardLoopControl::Continue => {
                dashboard_opts.welcome_banner = false;
                open_picker = false;
            }
        }
    }
}

fn global_dashboard_action_label(outcome: &TuiOutcome) -> &'static str {
    match outcome {
        TuiOutcome::Exited => "exit",
        TuiOutcome::NonTtyFallback => "non-TTY fallback",
        TuiOutcome::WizardCompleted => "wizard completion",
        TuiOutcome::WizardCancelled => "wizard cancellation",
        TuiOutcome::LaunchIntegrationRequested(_) => "agent integration setup",
        TuiOutcome::LaunchProjectMcpInstallRequested => "project MCP install",
        TuiOutcome::LaunchExplainSetupRequested => "explain setup",
        TuiOutcome::LaunchEmbeddingsSetupRequested => "embeddings setup",
        TuiOutcome::LaunchEmbeddingBuildRequested(_) => "embeddings build",
        TuiOutcome::SwitchProjectRequested(_) => "project switch",
    }
}

fn find_initialized_project_root(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        if Config::synrepo_dir(dir).join("config.toml").exists() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

fn has_git_ancestor(start: &Path) -> bool {
    start.ancestors().any(|dir| dir.join(".git").exists())
}

fn registry_has_projects() -> anyhow::Result<bool> {
    Ok(!registry::load()?.projects.is_empty())
}

/// Non-TTY plain-text summary printed when bare `synrepo` runs on a ready
/// repo behind a pipe or redirect. Mirrors the key lines from `synrepo status`.
pub(crate) fn bare_ready_summary(repo_root: &Path) -> anyhow::Result<String> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    if !synrepo_dir.exists() {
        anyhow::bail!(
            "repo is not initialized: {} is missing",
            synrepo_dir.display()
        );
    }
    super::commands::status_output(repo_root, false, false, false)
}

/// Explicit `synrepo dashboard`: probe, but exit non-zero on non-ready state
/// instead of routing to a wizard. Keeps scripted invocations deterministic.
pub(crate) fn run_dashboard_command(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<()> {
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
                "synrepo dashboard: repository is uninitialized. Run bare `synrepo` for guided setup, or `synrepo init --mode auto` for runtime-only bootstrap."
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

pub(crate) fn bare_uninitialized_fallback() -> String {
    "\
synrepo: this repository is not initialized.
Run bare `synrepo` in a TTY for guided setup.
For scripted setup, run `synrepo setup <tool>`.
For runtime-only bootstrap, run `synrepo init --mode auto`.
"
    .to_string()
}

pub(crate) fn bare_partial_fallback(missing: &[Missing]) -> String {
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

pub(crate) fn missing_label(m: &Missing) -> String {
    match m {
        Missing::ConfigFile => ".synrepo/config.toml missing".to_string(),
        Missing::ConfigUnreadable { detail } => format!("config.toml unreadable: {detail}"),
        Missing::GraphStore => ".synrepo/graph/nodes.db missing or not openable".to_string(),
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
