use std::path::Path;

use synrepo::bootstrap::runtime_probe::{probe, RoutingDecision};
use synrepo::tui::{
    run_explain_only_wizard, run_setup_wizard, stdout_is_tty, DashboardOptions, SetupPlan,
    SetupWizardOutcome, TuiOptions,
};

use super::agent_shims::{registry as shim_registry, AgentTool, AutomationTier};
use super::commands::{
    step_apply_explain, step_apply_integration, step_backup_mcp_config, step_ensure_ready,
    step_init,
};
use super::entry::{bare_ready_summary, bare_uninitialized_fallback};
use super::explain_cmd::print_explain_discovery_hint;
use super::repair_cmd::run_dashboard_with_sub_wizards;

/// Run the TUI setup wizard and apply its [`SetupPlan`] outcome. Shared by the
/// bare-entrypoint OpenSetup arm and the explicit `synrepo setup` command when
/// invoked without a `<tool>` argument. Caller is responsible for the non-TTY
/// short-circuit before calling -- this helper still handles the wizard's own
/// `NonTty` outcome (printed by the wizard itself) defensively.
pub(crate) fn run_wizard_and_apply(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<()> {
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

/// Execute a completed [`SetupPlan`] after the TUI alt-screen has been torn
/// down. All file-system writes happen here, not inside the library.
pub(crate) fn execute_setup_plan(repo_root: &Path, plan: SetupPlan) -> anyhow::Result<()> {
    println!("synrepo setup: applying plan.");
    step_init(repo_root, Some(plan.mode), false, false)?;
    if let Some(target) = plan.target {
        let tool = AgentTool::from_target_kind(target);
        let backup = step_backup_mcp_config(repo_root, tool)?;
        step_apply_integration(repo_root, tool, false, false)?;
        let wrote_mcp = matches!(tool.automation_tier(), AutomationTier::Automated);
        shim_registry::record_install_best_effort(repo_root, tool, wrote_mcp, backup);
    }
    if plan.explain.is_some() {
        step_apply_explain(repo_root, plan.explain.as_ref())?;
        print_explain_discovery_hint();
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

/// Launch the explain-only sub-wizard after `synrepo setup <tool> --explain`,
/// patching repo-local `.synrepo/config.toml` plus user-scoped
/// `~/.synrepo/config.toml` as needed. Non-TTY callers get a pointer to the
/// relevant config files instead of crashing.
pub(crate) fn run_explain_step(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<()> {
    match run_explain_only_wizard(opts)? {
        SetupWizardOutcome::Completed { plan } => {
            step_apply_explain(repo_root, plan.explain.as_ref())?;
            print_explain_discovery_hint();
            Ok(())
        }
        SetupWizardOutcome::Cancelled => {
            println!("explain sub-wizard cancelled; repo and user config untouched.");
            Ok(())
        }
        SetupWizardOutcome::NonTty => {
            println!(
                "--explain requires a TTY. Edit .synrepo/config.toml for repo-local \
                 enablement and ~/.synrepo/config.toml for reusable keys or local endpoints; \
                 see AGENTS.md for the `[explain]` block schema."
            );
            Ok(())
        }
    }
}

/// After a successful setup wizard, re-probe and open the dashboard with the
/// one-shot welcome banner seeded in the log pane. A partial re-classification
/// is unexpected here (setup just ran to completion), but we still fall
/// through gracefully rather than re-entering a wizard.
pub(crate) fn open_dashboard_after_wizard(
    repo_root: &Path,
    opts: TuiOptions,
) -> anyhow::Result<()> {
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
            // state). Surface the status summary when possible and fail
            // honestly so scripts do not treat the repo as operational.
            match bare_ready_summary(repo_root) {
                Ok(summary) => print!("{summary}"),
                Err(err) => eprintln!(
                    "Setup completed but the repo is not yet operational, and the status summary failed: {err:#}"
                ),
            }
            std::process::exit(2);
        }
    }
}
