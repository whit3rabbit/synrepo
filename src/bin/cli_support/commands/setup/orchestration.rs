use anyhow::anyhow;
use std::path::Path;

use super::report::{
    entry_after_failure, entry_after_success, render_client_setup_summary,
    render_detected_client_summary, ClientBefore, ClientSetupEntry,
};
use super::steps::setup;
use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::basic::agent_setup;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ToolResolution {
    pub selected: Vec<AgentTool>,
    pub skipped: Vec<AgentTool>,
}

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
#[cfg(test)]
pub(crate) fn resolve_tools(
    tool: Option<AgentTool>,
    only: &[AgentTool],
    skip: &[AgentTool],
) -> anyhow::Result<Vec<AgentTool>> {
    Ok(resolve_tool_resolution(tool, only, skip)?.selected)
}

pub(crate) fn resolve_tool_resolution(
    tool: Option<AgentTool>,
    only: &[AgentTool],
    skip: &[AgentTool],
) -> anyhow::Result<ToolResolution> {
    if tool.is_some() && (!only.is_empty() || !skip.is_empty()) {
        anyhow::bail!("positional tool cannot be combined with --only or --skip; pick one shape");
    }
    if let Some(tool) = tool {
        return Ok(ToolResolution {
            selected: vec![tool],
            skipped: vec![],
        });
    }
    if !only.is_empty() {
        let mut list = only.to_vec();
        dedup_preserve_order(&mut list);
        return Ok(ToolResolution {
            selected: list,
            skipped: vec![],
        });
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
        let mut skipped = skip.to_vec();
        dedup_preserve_order(&mut skipped);
        return Ok(ToolResolution { selected, skipped });
    }
    Err(anyhow!(
        "no target selected: pass a positional tool, `--only <tool,tool>`, or `--skip <tool,tool>`"
    ))
}

fn dedup_preserve_order(list: &mut Vec<AgentTool>) {
    let mut seen = std::collections::HashSet::new();
    list.retain(|tool| seen.insert(*tool));
}

fn run_many_with_skipped<F>(
    repo_root: &Path,
    tools: &[AgentTool],
    skipped: &[AgentTool],
    kind: &str,
    mut run_one: F,
) -> anyhow::Result<()>
where
    F: FnMut(AgentTool) -> anyhow::Result<()>,
{
    let detected = detected_tools(repo_root);
    print!(
        "{}",
        render_detected_client_summary(&detected, tools, skipped)
    );

    let mut entries: Vec<ClientSetupEntry> = skipped
        .iter()
        .copied()
        .map(|tool| ClientSetupEntry::skipped(repo_root, tool, detected.contains(&tool)))
        .collect();

    if let [single] = tools {
        let before = ClientBefore::observe(repo_root, *single);
        return match run_one(*single) {
            Ok(()) => {
                entries.push(entry_after_success(
                    repo_root,
                    *single,
                    before,
                    detected.contains(single),
                ));
                print!("{}", render_client_setup_summary(repo_root, kind, &entries));
                Ok(())
            }
            Err(err) => {
                entries.push(entry_after_failure(
                    repo_root,
                    *single,
                    detected.contains(single),
                    &err,
                ));
                print!("{}", render_client_setup_summary(repo_root, kind, &entries));
                Err(err)
            }
        };
    }
    let mut failures: Vec<(AgentTool, anyhow::Error)> = Vec::new();
    for (idx, tool) in tools.iter().copied().enumerate() {
        let before = ClientBefore::observe(repo_root, tool);
        println!(
            "\n=== [{}/{}] {} ===",
            idx + 1,
            tools.len(),
            tool.display_name()
        );
        match run_one(tool) {
            Ok(()) => entries.push(entry_after_success(
                repo_root,
                tool,
                before,
                detected.contains(&tool),
            )),
            Err(err) => {
                eprintln!("  error: {err:#}");
                entries.push(entry_after_failure(
                    repo_root,
                    tool,
                    detected.contains(&tool),
                    &err,
                ));
                failures.push((tool, err));
            }
        }
    }
    let succeeded = tools.len() - failures.len();
    println!(
        "\nmulti-client {kind}: {succeeded}/{total} succeeded",
        total = tools.len()
    );
    print!("{}", render_client_setup_summary(repo_root, kind, &entries));
    if !failures.is_empty() {
        let names: Vec<&'static str> = failures.iter().map(|(t, _)| t.display_name()).collect();
        anyhow::bail!("{kind} failed for: {}", names.join(", "));
    }
    Ok(())
}

fn detected_tools(repo_root: &Path) -> Vec<AgentTool> {
    synrepo::bootstrap::runtime_probe::probe(repo_root)
        .detected_agent_targets
        .into_iter()
        .map(AgentTool::from_target_kind)
        .collect()
}

pub(crate) fn setup_many_resolved(
    repo_root: &Path,
    resolution: &ToolResolution,
    force: bool,
    gitignore: bool,
) -> anyhow::Result<()> {
    run_many_with_skipped(
        repo_root,
        &resolution.selected,
        &resolution.skipped,
        "setup",
        |tool| setup(repo_root, tool, force, gitignore),
    )
}

pub(crate) fn agent_setup_many_resolved(
    repo_root: &Path,
    resolution: &ToolResolution,
    force: bool,
    regen: bool,
) -> anyhow::Result<()> {
    run_many_with_skipped(
        repo_root,
        &resolution.selected,
        &resolution.skipped,
        "agent-setup",
        |tool| agent_setup(repo_root, tool, force, regen),
    )
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
