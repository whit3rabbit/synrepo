use anyhow::anyhow;
use std::path::Path;

use super::steps::setup;
use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::basic::agent_setup;

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
