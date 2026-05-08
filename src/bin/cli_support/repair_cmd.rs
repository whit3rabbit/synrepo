use std::io::{self, Write};
use std::path::Path;

use crossterm::{
    cursor::Show,
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use synrepo::bootstrap::runtime_probe::{probe, AgentIntegration};
use synrepo::tui::{
    run_dashboard, run_integration_wizard, run_mcp_install_wizard, DashboardOptions,
    IntegrationPlan, IntegrationWizardOutcome, McpInstallPlan, McpInstallWizardOutcome, RepairPlan,
    TuiOptions, TuiOutcome,
};

use super::agent_shims::{registry as shim_registry, AgentTool, AutomationTier};
use super::commands::{
    embeddings_build_human, reconcile, resolve_setup_scope, step_apply_integration,
    step_backup_mcp_config, step_init, step_install_agent_hooks, step_register_mcp,
    step_write_shim, StepOutcome,
};
use super::setup_cmd::{run_embeddings_setup_step, run_explain_step};

/// Run the poll-mode dashboard in a loop so dashboard-launched sub-wizards can
/// tear down the alt-screen, execute their plan, and re-open the dashboard
/// with a fresh probe and integration signal. Returns once the operator quits
/// the dashboard normally or a non-TTY fallback fires.
pub(crate) fn run_dashboard_with_sub_wizards(
    repo_root: &Path,
    mut integration: AgentIntegration,
    mut opts: DashboardOptions,
) -> anyhow::Result<()> {
    let mut current_root = repo_root.to_path_buf();
    loop {
        // Exhaustive match flags future TuiOutcome additions at compile time.
        match run_dashboard(&current_root, integration.clone(), opts)? {
            TuiOutcome::Exited | TuiOutcome::NonTtyFallback => return Ok(()),
            TuiOutcome::SwitchProjectRequested(next_root) => {
                current_root = next_root;
                let report = probe(&current_root);
                integration = report.agent_integration;
                opts.welcome_banner = false;
            }
            TuiOutcome::LaunchIntegrationRequested => {
                // Tear-down of the alt-screen has already happened inside
                // `run_dashboard`; safe to print and prompt now.
                let tui_opts = TuiOptions {
                    no_color: opts.no_color,
                };
                match run_integration_wizard(&current_root, integration.clone(), tui_opts)? {
                    IntegrationWizardOutcome::Completed { plan } => {
                        execute_integration_plan(&current_root, plan)?;
                    }
                    IntegrationWizardOutcome::Cancelled => {
                        println!("integration wizard cancelled; no changes applied.");
                    }
                    IntegrationWizardOutcome::NonTty => return Ok(()),
                }
                // Re-probe so the dashboard reflects the new integration
                // state on re-open. Suppress the welcome banner on re-open —
                // the banner is a first-run-only affordance.
                let report = probe(&current_root);
                integration = report.agent_integration;
                opts.welcome_banner = false;
            }
            TuiOutcome::LaunchProjectMcpInstallRequested => {
                let tui_opts = TuiOptions {
                    no_color: opts.no_color,
                };
                match run_mcp_install_wizard(&current_root, tui_opts)? {
                    McpInstallWizardOutcome::Completed { plan } => {
                        execute_project_mcp_install_plan(&current_root, plan)?;
                    }
                    McpInstallWizardOutcome::Cancelled => {
                        println!("repo MCP install cancelled; no changes applied.");
                    }
                    McpInstallWizardOutcome::NonTty => return Ok(()),
                }
                let report = probe(&current_root);
                integration = report.agent_integration;
                opts.welcome_banner = false;
            }
            TuiOutcome::LaunchExplainSetupRequested => {
                let tui_opts = TuiOptions {
                    no_color: opts.no_color,
                };
                run_explain_step(&current_root, tui_opts)?;
                opts.welcome_banner = false;
            }
            TuiOutcome::LaunchEmbeddingsSetupRequested => {
                let tui_opts = TuiOptions {
                    no_color: opts.no_color,
                };
                run_embeddings_setup_step(&current_root, tui_opts)?;
                opts.welcome_banner = false;
            }
            TuiOutcome::LaunchEmbeddingBuildRequested(pending) => {
                run_embedding_build_step(&current_root, pending)?;
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

fn run_embedding_build_step(
    repo_root: &Path,
    pending: synrepo::tui::app::PendingEmbeddingBuild,
) -> anyhow::Result<()> {
    restore_normal_terminal_for_dashboard_handoff();
    if pending.stopped_watch {
        println!("Watch stopped. Building embeddings in normal terminal output...");
    } else {
        println!("Building embeddings in normal terminal output...");
    }
    if let Err(error) = embeddings_build_human(repo_root) {
        eprintln!("embeddings build failed: {error:#}");
    }
    wait_for_enter_if_tty()
}

fn wait_for_enter_if_tty() -> anyhow::Result<()> {
    if !synrepo::tui::stdout_is_tty() {
        return Ok(());
    }
    restore_normal_terminal_for_dashboard_handoff();
    print!("Press Enter to reopen the dashboard...");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(())
}

fn restore_normal_terminal_for_dashboard_handoff() {
    if !synrepo::tui::stdout_is_tty() {
        return;
    }
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, Show);
    let _ = write!(stdout, "\r");
    let _ = stdout.flush();
}

/// Execute a completed [`RepairPlan`] after the TUI alt-screen has been torn
/// down. Actions run in order: write config, recreate runtime, reconcile,
/// shim. The probe is re-run between mutating steps so later steps see fresh
/// state and a transient success transitions cleanly to the dashboard on the
/// next bare-`synrepo` run.
pub(crate) fn execute_repair_plan(repo_root: &Path, plan: RepairPlan) -> anyhow::Result<()> {
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
    if plan.recreate_runtime {
        println!("  Recreating .synrepo/ via `init --force`...");
        // `synrepo upgrade --apply` cannot migrate a canonical-store Block;
        // `init --force` clears the blocked stores under the writer lock and
        // re-runs bootstrap.
        step_init(repo_root, None, true, false)?;
        let _ = probe(repo_root);
    }
    if plan.run_reconcile {
        println!("  Running reconcile pass...");
        reconcile(repo_root, false)?;
        let _ = probe(repo_root);
    }
    if let Some(target) = plan.write_shim_for {
        let tool = AgentTool::from_target_kind(target);
        println!(
            "  Writing {} {}...",
            tool.display_name(),
            tool.artifact_label()
        );
        let scope = resolve_setup_scope(repo_root, tool, false);
        let backup = if matches!(scope, agent_config::Scope::Local(_)) {
            step_backup_mcp_config(repo_root, tool, &scope)?
        } else {
            None
        };
        step_apply_integration(repo_root, tool, false, &scope)?;
        let wrote_mcp = matches!(tool.automation_tier(), AutomationTier::Automated);
        shim_registry::record_install_best_effort(repo_root, tool, &scope, wrote_mcp, backup);
    }
    println!("Repair complete.");
    Ok(())
}

/// Execute a completed [`IntegrationPlan`] after the TUI alt-screen has been
/// torn down. Splits the plan so the wizard can request individual actions,
/// while still keeping MCP registration paired with its skill/instruction.
pub(crate) fn execute_integration_plan(
    repo_root: &Path,
    plan: IntegrationPlan,
) -> anyhow::Result<()> {
    let tool = AgentTool::from_target_kind(plan.target);
    let scope = resolve_setup_scope(repo_root, tool, false);
    if plan.write_shim {
        step_write_shim(repo_root, tool, &scope, plan.overwrite_shim)?;
    }
    let mut backup: Option<String> = None;
    if plan.register_mcp {
        if !plan.write_shim {
            step_write_shim(repo_root, tool, &scope, plan.overwrite_shim)?;
        }
        if matches!(scope, agent_config::Scope::Local(_)) {
            backup = step_backup_mcp_config(repo_root, tool, &scope)?;
        }
        step_register_mcp(repo_root, tool, &scope)?;
    }
    if plan.install_agent_hooks {
        step_install_agent_hooks(repo_root, tool)?;
    }
    let wrote_mcp =
        plan.register_mcp && matches!(tool.automation_tier(), AutomationTier::Automated);
    shim_registry::record_install_best_effort(repo_root, tool, &scope, wrote_mcp, backup);
    println!("Integration complete.");
    Ok(())
}

/// Execute a completed repo-local MCP install plan from the dashboard MCP tab.
/// Project-scoped MCP is paired with the target's project skill/instruction so
/// the agent gets both tools and the guidance for when to use them.
pub(crate) fn execute_project_mcp_install_plan(
    repo_root: &Path,
    plan: McpInstallPlan,
) -> anyhow::Result<()> {
    let tool = AgentTool::from_agent_config_id(&plan.target)
        .ok_or_else(|| anyhow::anyhow!("unsupported agent-config target: {}", plan.target))?;
    let scope = agent_config::Scope::Local(repo_root.to_path_buf());
    println!(
        "Installing repo-local synrepo MCP for {}...",
        tool.display_name()
    );
    step_write_shim(repo_root, tool, &scope, false)?;
    let backup = step_backup_mcp_config(repo_root, tool, &scope)?;
    match step_register_mcp(repo_root, tool, &scope)? {
        StepOutcome::NotAutomated => anyhow::bail!(
            "{} does not support automated repo-local MCP registration",
            tool.display_name()
        ),
        StepOutcome::Applied | StepOutcome::AlreadyCurrent | StepOutcome::Updated => {}
    }
    shim_registry::record_install_best_effort(repo_root, tool, &scope, true, backup);
    println!("Repo MCP install complete.");
    Ok(())
}
