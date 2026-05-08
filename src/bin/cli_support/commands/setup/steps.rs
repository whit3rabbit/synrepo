use agent_config::{Scope, ScopeKind};
use std::fs;
use std::path::Path;
use synrepo::config::{Config, Mode};

use super::mcp_register;
use crate::cli_support::agent_shims::{
    registry as shim_registry, scope_label, AgentTool, AutomationTier,
};
use crate::cli_support::commands::agent_hooks;
use crate::cli_support::commands::basic::agent_setup_with_scope;
use crate::cli_support::commands::setup_mcp_backup::step_backup_mcp_config;

/// Outcome of a single setup step. Tests assert on this rather than captured
/// stdout; the CLI still prints progress lines for user-visible output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StepOutcome {
    /// Step performed a new write.
    Applied,
    /// Step was a no-op; existing state already matched the target.
    AlreadyCurrent,
    /// Step updated an existing value (present but different).
    Updated,
    /// Automation is not implemented for the given target.
    NotAutomated,
}

/// Full onboarding flow for a specific agent client. Thin composer over the
/// decomposed `step_*` helpers so TUI wizards can reuse the same steps.
pub(crate) fn setup(
    repo_root: &Path,
    tool: AgentTool,
    force: bool,
    gitignore: bool,
    project: bool,
    agent_hooks_enabled: bool,
) -> anyhow::Result<()> {
    let scope = resolve_setup_scope(repo_root, tool, project);
    println!(
        "Setting up synrepo for {} ({})...",
        tool.display_name(),
        scope_label(&scope)
    );
    if agent_hooks_enabled {
        println!("  Hooks: installing local synrepo nudge hooks.");
    } else {
        println!("  Hooks: not installed; pass --agent-hooks to add local nudges.");
    }

    step_init(repo_root, None, force, gitignore)?;
    // Back up the tool's MCP config before any mutation so `synrepo remove`
    // can preserve the stored path as a `.bak` sidecar.
    let backup = match &scope {
        Scope::Local(_) => step_backup_mcp_config(repo_root, tool, &scope)?,
        Scope::Global => None,
        _ => None,
    };
    step_apply_integration(repo_root, tool, force, &scope)?;
    if agent_hooks_enabled {
        step_install_agent_hooks(repo_root, tool)?;
    }
    step_ensure_ready(repo_root)?;
    synrepo::registry::record_project(repo_root)?;

    let wrote_mcp = matches!(tool.automation_tier(), AutomationTier::Automated);
    shim_registry::record_install_best_effort(repo_root, tool, &scope, wrote_mcp, backup);

    println!("\nSetup complete. Repo is ready. One Next Step:");
    match tool {
        AgentTool::Claude => {
            println!("  Run `claude` and it will automatically load the synrepo MCP server.")
        }
        AgentTool::Codex => {
            println!("  Run `codex` and it will automatically load the synrepo MCP server.")
        }
        AgentTool::OpenCode => {
            println!("  OpenCode will automatically load the synrepo MCP server and AGENTS.md.")
        }
        AgentTool::Cursor => {
            println!(
                "  Open Cursor in this repo and it will load the configured synrepo MCP server."
            )
        }
        AgentTool::Windsurf => {
            println!(
                "  Open Windsurf in this repo and it will load the configured synrepo MCP server."
            )
        }
        AgentTool::Roo => {
            println!(
                "  Open Roo Code in this repo and it will load the configured synrepo MCP server."
            )
        }
        other => {
            // Shim-only tier: the shim is written, but MCP registration is
            // manual. Give the operator the concrete follow-ups they need.
            debug_assert_eq!(other.automation_tier(), AutomationTier::ShimOnly);
            println!("  Shim written: {}", other.output_path(repo_root).display());
            println!("  Next: {}", other.include_instruction());
            println!("  MCP server: point your agent at `synrepo mcp --repo .` (stdio transport).");
        }
    }

    Ok(())
}

pub(crate) fn resolve_setup_scope(repo_root: &Path, tool: AgentTool, project: bool) -> Scope {
    if project {
        return Scope::Local(repo_root.to_path_buf());
    }
    let scopes = tool.supported_scopes();
    if scopes.contains(&ScopeKind::Global) {
        Scope::Global
    } else if scopes.contains(&ScopeKind::Local) {
        println!(
            "  {} does not support global MCP registration; falling back to project scope.",
            tool.display_name()
        );
        Scope::Local(repo_root.to_path_buf())
    } else {
        Scope::Local(repo_root.to_path_buf())
    }
}

/// Initialize `.synrepo/` if not present (or always with `force`). Returns
/// `AlreadyCurrent` when the directory is present and `force` is false.
///
/// When `force = true` the call is forwarded to `init(force=true)`, which
/// also unblocks a runtime whose canonical graph store is incompatible with
/// the current binary. This is the path the repair wizard's
/// `RecreateRuntime` action takes.
pub(crate) fn step_init(
    repo_root: &Path,
    mode: Option<Mode>,
    force: bool,
    gitignore: bool,
) -> anyhow::Result<StepOutcome> {
    step_init_with_config(repo_root, mode, force, gitignore, |_| {})
}

/// Initialize `.synrepo/` with a setup-specific config mutation applied before
/// bootstrap writes config and builds indexes.
pub(crate) fn step_init_with_config<F>(
    repo_root: &Path,
    mode: Option<Mode>,
    force: bool,
    gitignore: bool,
    configure_config: F,
) -> anyhow::Result<StepOutcome>
where
    F: FnOnce(&mut Config),
{
    let synrepo_dir = repo_root.join(".synrepo");
    if !synrepo_dir.exists() || force {
        println!("  Initializing .synrepo/...");
        let report = synrepo::bootstrap::bootstrap_with_force_and_config(
            repo_root,
            mode,
            gitignore,
            force,
            configure_config,
        )?;
        print!("{}", report.render());
        Ok(StepOutcome::Applied)
    } else {
        println!("  .synrepo/ already initialized.");
        Ok(StepOutcome::AlreadyCurrent)
    }
}

/// Write the agent integration shim for `target`.
///
/// Missing shims are always written. Existing shims are preserved unless the
/// caller explicitly opts into overwrite behavior, in which case the helper
/// reuses `agent_setup(..., force = true, regen = true)` to refresh stale
/// content without blindly rewriting identical files.
pub(crate) fn step_write_shim(
    repo_root: &Path,
    target: AgentTool,
    scope: &Scope,
    overwrite: bool,
) -> anyhow::Result<StepOutcome> {
    let out_path = target.output_path(repo_root);
    println!(
        "  Writing {} {}...",
        target.display_name(),
        target.artifact_label()
    );

    if matches!(scope, Scope::Global) {
        agent_setup_with_scope(repo_root, target, scope, overwrite, overwrite)?;
        return Ok(StepOutcome::Applied);
    }

    if !out_path.exists() {
        agent_setup_with_scope(repo_root, target, scope, false, false)?;
        return Ok(StepOutcome::Applied);
    }

    if !overwrite {
        println!(
            "  Existing {} {} preserved: overwrite not requested.",
            target.display_name(),
            target.artifact_label()
        );
        return Ok(StepOutcome::AlreadyCurrent);
    }

    let was_current = fs::read_to_string(&out_path)
        .map(|existing| existing == target.shim_content())
        .unwrap_or(false);
    agent_setup_with_scope(repo_root, target, scope, true, true)?;
    Ok(if was_current {
        StepOutcome::AlreadyCurrent
    } else {
        StepOutcome::Updated
    })
}

/// Register the synrepo MCP server in the target agent's project config.
/// Returns `NotAutomated` for targets without scripted registration.
pub(crate) fn step_register_mcp(
    repo_root: &Path,
    target: AgentTool,
    scope: &Scope,
) -> anyhow::Result<StepOutcome> {
    step_register_mcp_with_force(repo_root, target, scope, false)
}

pub(crate) fn step_register_mcp_with_force(
    repo_root: &Path,
    target: AgentTool,
    scope: &Scope,
    force_adopt_unowned: bool,
) -> anyhow::Result<StepOutcome> {
    if target.installer_supports_mcp() {
        return mcp_register::register_synrepo_mcp(
            repo_root,
            target,
            scope.clone(),
            force_adopt_unowned,
        );
    }
    debug_assert_eq!(target.automation_tier(), AutomationTier::ShimOnly);
    println!(
        "  {} uses instructions-only integration; register `synrepo mcp --repo .` \
         as a stdio MCP server in the agent's own config.",
        target.display_name()
    );
    Ok(StepOutcome::NotAutomated)
}

/// Composite integration step: write the shim, then register the MCP server.
pub(crate) fn step_apply_integration(
    repo_root: &Path,
    target: AgentTool,
    force: bool,
    scope: &Scope,
) -> anyhow::Result<StepOutcome> {
    let shim = step_write_shim(repo_root, target, scope, force)?;
    let mcp = step_register_mcp_with_force(repo_root, target, scope, force)?;
    Ok(match (shim, mcp) {
        (StepOutcome::Applied | StepOutcome::Updated, _) => StepOutcome::Applied,
        (_, StepOutcome::Applied) | (_, StepOutcome::Updated) => StepOutcome::Applied,
        (_, StepOutcome::NotAutomated) => StepOutcome::NotAutomated,
        _ => StepOutcome::AlreadyCurrent,
    })
}

pub(crate) fn step_install_agent_hooks(
    repo_root: &Path,
    target: AgentTool,
) -> anyhow::Result<StepOutcome> {
    if !agent_hooks::agent_hooks_supported(target) {
        anyhow::bail!(
            "{} does not support synrepo agent nudge hooks",
            target.display_name()
        );
    }
    agent_hooks::install_agent_hooks(repo_root, target)
}

/// Ensure setup leaves an operationally ready runtime by creating the first
/// reconcile state when it is still missing after init.
pub(crate) fn step_ensure_ready(repo_root: &Path) -> anyhow::Result<StepOutcome> {
    let state_path = repo_root
        .join(".synrepo")
        .join("state")
        .join("reconcile-state.json");
    if state_path.exists() {
        println!("  Reconcile state already present.");
        return Ok(StepOutcome::AlreadyCurrent);
    }

    println!("  Running first reconcile pass...");
    crate::cli_support::commands::repair::reconcile(repo_root, false)?;
    Ok(StepOutcome::Applied)
}
