use std::fs;
use std::path::Path;
use synrepo::config::Mode;

use super::mcp_register;
use crate::cli_support::agent_shims::{registry as shim_registry, AgentTool, AutomationTier};
use crate::cli_support::commands::basic::{agent_setup, init};
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
) -> anyhow::Result<()> {
    println!("Setting up synrepo for {}...", tool.display_name());

    step_init(repo_root, None, force, gitignore)?;
    // Back up the tool's MCP config before any mutation so `synrepo remove`
    // can preserve the stored path as a `.bak` sidecar.
    let backup = step_backup_mcp_config(repo_root, tool)?;
    step_apply_integration(repo_root, tool, force)?;
    step_ensure_ready(repo_root)?;

    let wrote_mcp = matches!(tool.automation_tier(), AutomationTier::Automated);
    shim_registry::record_install_best_effort(repo_root, tool, wrote_mcp, backup);

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
                "  Cursor will automatically load the synrepo MCP server from .cursor/mcp.json."
            )
        }
        AgentTool::Windsurf => {
            println!(
                "  Windsurf will automatically load the synrepo MCP server from .windsurf/mcp.json."
            )
        }
        AgentTool::Roo => {
            println!(
                "  Roo Code will automatically load the synrepo MCP server from .roo/mcp.json."
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

/// Initialize `.synrepo/` if not present (or always with `force`). Returns
/// `AlreadyCurrent` when the directory is present and `force` is false.
pub(crate) fn step_init(
    repo_root: &Path,
    mode: Option<Mode>,
    force: bool,
    gitignore: bool,
) -> anyhow::Result<StepOutcome> {
    let synrepo_dir = repo_root.join(".synrepo");
    if !synrepo_dir.exists() || force {
        println!("  Initializing .synrepo/...");
        init(repo_root, mode, gitignore)?;
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
    overwrite: bool,
) -> anyhow::Result<StepOutcome> {
    let out_path = target.output_path(repo_root);
    println!(
        "  Writing {} {}...",
        target.display_name(),
        target.artifact_label()
    );

    if !out_path.exists() {
        agent_setup(repo_root, target, false, false)?;
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
    agent_setup(repo_root, target, true, true)?;
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
) -> anyhow::Result<StepOutcome> {
    match target {
        AgentTool::Claude => mcp_register::setup_claude_mcp(repo_root),
        AgentTool::Codex => mcp_register::setup_codex_mcp(repo_root),
        AgentTool::OpenCode => mcp_register::setup_opencode_mcp(repo_root),
        AgentTool::Cursor => mcp_register::setup_cursor_mcp(repo_root),
        AgentTool::Windsurf => mcp_register::setup_windsurf_mcp(repo_root),
        AgentTool::Roo => mcp_register::setup_roo_mcp(repo_root),
        other => {
            debug_assert_eq!(other.automation_tier(), AutomationTier::ShimOnly);
            println!(
                "  {} uses instructions-only integration; register `synrepo mcp --repo .` \
                 as a stdio MCP server in the agent's own config.",
                other.display_name()
            );
            Ok(StepOutcome::NotAutomated)
        }
    }
}

/// Composite integration step: write the shim, then register the MCP server.
pub(crate) fn step_apply_integration(
    repo_root: &Path,
    target: AgentTool,
    force: bool,
) -> anyhow::Result<StepOutcome> {
    let shim = step_write_shim(repo_root, target, force)?;
    let mcp = step_register_mcp(repo_root, target)?;
    Ok(match (shim, mcp) {
        (StepOutcome::Applied | StepOutcome::Updated, _) => StepOutcome::Applied,
        (_, StepOutcome::Applied) | (_, StepOutcome::Updated) => StepOutcome::Applied,
        (_, StepOutcome::NotAutomated) => StepOutcome::NotAutomated,
        _ => StepOutcome::AlreadyCurrent,
    })
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
    crate::cli_support::commands::repair::reconcile(repo_root)?;
    Ok(StepOutcome::Applied)
}
