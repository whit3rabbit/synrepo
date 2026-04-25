use std::path::Path;

use synrepo::bootstrap::runtime_probe::{probe, Missing, RoutingDecision};
use synrepo::config::Config;
use synrepo::tui::{
    run_repair_wizard, stdout_is_tty, DashboardOptions, RepairWizardOutcome, TuiOptions,
};

use super::repair_cmd::{execute_repair_plan, run_dashboard_with_sub_wizards};
use super::setup_cmd::run_wizard_and_apply;

/// Bare `synrepo`: probe, route, and run the appropriate TUI entrypoint.
pub(crate) fn run_bare_entrypoint(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<()> {
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

pub(crate) fn bare_uninitialized_fallback() -> String {
    "\
synrepo: this repository is not initialized.
Run `synrepo init` to create .synrepo/ and populate the graph.
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
