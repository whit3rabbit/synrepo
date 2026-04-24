use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use synrepo::config::Mode;

use super::basic::{agent_setup, init};
use super::setup_mcp_backup::step_backup_mcp_config;
use crate::cli_support::agent_shims::{registry as shim_registry, AgentTool, AutomationTier};

mod mcp_register;

#[cfg(test)]
pub(crate) use mcp_register::{
    setup_claude_mcp, setup_codex_mcp, setup_cursor_mcp, setup_opencode_mcp, setup_roo_mcp,
    setup_windsurf_mcp,
};

/// Resolve the list of agent targets from the three CLI shapes we accept:
/// a single positional `tool`, `--only <tool,tool>`, or `--skip <tool,tool>`.
///
/// Clap enforces the three-way mutual exclusion at parse time (positional vs.
/// list via `conflicts_with_all`, `--only` vs. `--skip` via `conflicts_with`),
/// so callers should never reach here with conflicting shapes. The defensive
/// rejection below keeps the invariant enforced for direct callers (tests,
/// internal fan-out) in case the clap annotations drift.
///
/// `--skip` expands to every known `AgentTool` minus the skipped set.
pub(crate) fn resolve_tools(
    tool: Option<AgentTool>,
    only: &[AgentTool],
    skip: &[AgentTool],
) -> anyhow::Result<Vec<AgentTool>> {
    if tool.is_some() && (!only.is_empty() || !skip.is_empty()) {
        anyhow::bail!("positional tool cannot be combined with --only or --skip; pick one shape");
    }
    if let Some(tool) = tool {
        return Ok(vec![tool]);
    }
    if !only.is_empty() {
        let mut list = only.to_vec();
        dedup_preserve_order(&mut list);
        return Ok(list);
    }
    if !skip.is_empty() {
        use clap::ValueEnum;
        let skip_set: std::collections::HashSet<AgentTool> = skip.iter().copied().collect();
        let selected: Vec<AgentTool> = AgentTool::value_variants()
            .iter()
            .copied()
            .filter(|variant| !skip_set.contains(variant))
            .collect();
        if selected.is_empty() {
            anyhow::bail!("--skip excludes every known target; nothing to set up");
        }
        return Ok(selected);
    }
    Err(anyhow!(
        "no target selected: pass a positional tool, `--only <tool,tool>`, or `--skip <tool,tool>`"
    ))
}

fn dedup_preserve_order(list: &mut Vec<AgentTool>) {
    let mut seen = std::collections::HashSet::new();
    list.retain(|tool| seen.insert(*tool));
}

/// Shared fan-out over a resolved list of agent targets. For a single target
/// the per-tool closure is invoked directly so its error surfaces unchanged.
/// For multiple targets, per-tool failures are collected and reported at the
/// end; the batch never short-circuits on a single failing target. `kind`
/// only labels the header and summary text, so both `setup` and
/// `agent-setup` can share the same progress shape.
fn run_many<F>(tools: &[AgentTool], kind: &str, mut run_one: F) -> anyhow::Result<()>
where
    F: FnMut(AgentTool) -> anyhow::Result<()>,
{
    if let [single] = tools {
        return run_one(*single);
    }
    let mut failures: Vec<(AgentTool, anyhow::Error)> = Vec::new();
    for (idx, tool) in tools.iter().copied().enumerate() {
        println!(
            "\n=== [{}/{}] {} ===",
            idx + 1,
            tools.len(),
            tool.display_name()
        );
        if let Err(err) = run_one(tool) {
            eprintln!("  error: {err:#}");
            failures.push((tool, err));
        }
    }
    let succeeded = tools.len() - failures.len();
    println!(
        "\nmulti-client {kind}: {succeeded}/{total} succeeded",
        total = tools.len()
    );
    if !failures.is_empty() {
        let names: Vec<&'static str> = failures.iter().map(|(t, _)| t.display_name()).collect();
        anyhow::bail!("{kind} failed for: {}", names.join(", "));
    }
    Ok(())
}

/// Run `setup` across a resolved list of agent targets. Init and first reconcile
/// happen once (via the first `setup()` call's idempotent steps); later tools
/// reuse the now-initialized `.synrepo/` and only perform shim + MCP work.
pub(crate) fn setup_many(
    repo_root: &Path,
    tools: &[AgentTool],
    force: bool,
    gitignore: bool,
) -> anyhow::Result<()> {
    run_many(tools, "setup", |tool| {
        setup(repo_root, tool, force, gitignore)
    })
}

/// Run `agent_setup` across a resolved list of agent targets. Shares its
/// progress shape with [`setup_many`] so operators see a consistent multi-
/// client view across both commands.
pub(crate) fn agent_setup_many(
    repo_root: &Path,
    tools: &[AgentTool],
    force: bool,
    regen: bool,
) -> anyhow::Result<()> {
    run_many(tools, "agent-setup", |tool| {
        agent_setup(repo_root, tool, force, regen)
    })
}

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
    super::repair::reconcile(repo_root)?;
    Ok(StepOutcome::Applied)
}

/// Parse a JSON file if it exists; fail loud with the file path if the content
/// is present but malformed, rather than silently discarding user config.
pub(crate) fn load_json_config(path: &Path) -> anyhow::Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(&content).map_err(|err| {
        anyhow!(
            "refusing to overwrite {}: file exists but is not valid JSON ({err}). \
             Fix or remove the file and re-run `synrepo setup`.",
            path.display()
        )
    })
}

/// Write JSON back to disk with pretty-printing and a trailing newline.
pub(crate) fn write_json_config(path: &Path, value: &Value) -> anyhow::Result<()> {
    let mut out = serde_json::to_string_pretty(value)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    out.push('\n');
    write_atomic(path, out.as_bytes())
}

fn write_atomic(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    synrepo::util::atomic_write(path, contents)
        .with_context(|| format!("failed to atomically write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positional_tool_resolves_to_single_target() {
        let out = resolve_tools(Some(AgentTool::Claude), &[], &[]).expect("ok");
        assert_eq!(out, vec![AgentTool::Claude]);
    }

    #[test]
    fn only_expands_and_dedups_preserving_order() {
        let out = resolve_tools(
            None,
            &[AgentTool::Claude, AgentTool::Cursor, AgentTool::Claude],
            &[],
        )
        .expect("ok");
        assert_eq!(out, vec![AgentTool::Claude, AgentTool::Cursor]);
    }

    #[test]
    fn skip_removes_excluded_from_every_known_target() {
        use clap::ValueEnum;
        let out = resolve_tools(None, &[], &[AgentTool::Copilot]).expect("ok");
        assert!(
            !out.contains(&AgentTool::Copilot),
            "skipped target must be absent"
        );
        assert_eq!(
            out.len(),
            AgentTool::value_variants().len() - 1,
            "skip drops exactly one target from the full roster"
        );
    }

    #[test]
    fn skip_cannot_exclude_everything() {
        use clap::ValueEnum;
        let all: Vec<AgentTool> = AgentTool::value_variants().to_vec();
        let err = resolve_tools(None, &[], &all).unwrap_err().to_string();
        assert!(
            err.contains("nothing to set up"),
            "excluding all targets should error loudly, got: {err}"
        );
    }

    #[test]
    fn positional_conflicts_with_only() {
        let err = resolve_tools(Some(AgentTool::Claude), &[AgentTool::Cursor], &[])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("positional tool cannot be combined"),
            "positional + --only must error, got: {err}"
        );
    }

    #[test]
    fn positional_conflicts_with_skip() {
        let err = resolve_tools(Some(AgentTool::Claude), &[], &[AgentTool::Cursor])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("positional tool cannot be combined"),
            "positional + --skip must error, got: {err}"
        );
    }

    #[test]
    fn no_selection_is_an_error() {
        let err = resolve_tools(None, &[], &[]).unwrap_err().to_string();
        assert!(
            err.contains("no target selected"),
            "empty selection must error, got: {err}"
        );
    }
}
