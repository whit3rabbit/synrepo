//! `synrepo remove` — uninstall synrepo artifacts from the current repo.
//!
//! Two surfaces share the plan builder here:
//!
//! - `synrepo remove <tool>` narrows the plan to one agent's shim + MCP entry.
//! - `synrepo remove` (bulk) includes every tracked/detected agent, the root
//!   `.gitignore` line (only if we added it), and the `.synrepo/` directory.
//!
//! Both paths default to a dry-run: `--apply` actually mutates the filesystem.
//! The TUI wizard (Phase 5) will construct a [`RemovePlan`] from the same
//! sources and feed it to [`apply_plan`].

mod apply;
mod plan;
mod wizard;

use std::path::{Path, PathBuf};

use serde::Serialize;
use synrepo::config::Config;
use synrepo::pipeline::watch::{watch_service_status, WatchServiceStatus};
use synrepo::registry;
use synrepo::tui::{run_uninstall_wizard, stdout_is_tty, TuiOptions, UninstallWizardOutcome};

use crate::cli_support::agent_shims::AgentTool;

pub(crate) use apply::apply_plan;
pub(crate) use plan::build_plan;
use wizard::{apply_wizard_plan, to_uninstall_kinds, wizard_plan_to_remove_plan};

/// One action in a removal plan.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum RemoveAction {
    /// Delete an agent shim file.
    DeleteShim {
        tool: String,
        /// Absolute path to the shim file.
        path: PathBuf,
    },
    /// Remove the synrepo entry from an MCP config file without deleting
    /// the file itself. The file is left alone if synrepo is its only entry
    /// (we write `{}` or equivalent rather than deleting).
    StripMcpEntry {
        tool: String,
        /// Absolute path to the MCP config file.
        path: PathBuf,
    },
    /// Strip a line we appended to the root `.gitignore`.
    RemoveGitignoreLine {
        /// The literal line, e.g. `.synrepo/`.
        entry: String,
    },
    /// Delete the `.synrepo/` directory and everything inside it.
    DeleteSynrepoDir,
}

/// A concrete removal plan. Built once, rendered for `--json` / dry-run, and
/// executed by [`apply_plan`] when `--apply` is set or the TUI confirms.
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct RemovePlan {
    pub actions: Vec<RemoveAction>,
    /// Paths the plan explicitly preserves (`.mcp.json.bak` sidecars). Surfaced
    /// in the dry-run table so users know the backups are intentionally kept.
    pub preserved: Vec<PathBuf>,
}

impl RemovePlan {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

/// Per-action outcome for the post-apply summary.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct AppliedAction {
    #[serde(flatten)]
    pub action: RemoveAction,
    pub succeeded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct ApplySummary {
    pub applied: Vec<AppliedAction>,
}

/// CLI entry point. Builds the plan, renders it (table or JSON), and applies
/// only when `apply` is set.
///
/// The bulk dry-run path (`tool.is_none() && !apply && !json`) launches the
/// TUI uninstall wizard on a TTY: the operator toggles rows, confirms, and the
/// resulting plan is applied in place (no second `--apply` required). Non-TTY
/// invocation falls through to the plain-text plan table.
pub(crate) fn remove(
    repo_root: &Path,
    tool: Option<AgentTool>,
    apply: bool,
    json: bool,
    keep_synrepo_dir: bool,
    force: bool,
) -> anyhow::Result<()> {
    let plan = build_plan(repo_root, tool, keep_synrepo_dir)?;

    // Bulk-remove on a TTY with no `--apply`/`--json` goes through the wizard.
    // The wizard serves as both the plan display and the confirm step; its
    // returned plan replaces the built one and is applied immediately.
    let wizard_consent =
        !apply && !json && tool.is_none() && !plan.is_empty() && stdout_is_tty() && !force;
    if wizard_consent {
        let installed = to_uninstall_kinds(&plan.actions);
        match run_uninstall_wizard(installed, plan.preserved.clone(), TuiOptions::default())? {
            UninstallWizardOutcome::NonTty => {
                // Fallthrough: render table, dry-run hint.
            }
            UninstallWizardOutcome::Cancelled => {
                println!("uninstall wizard cancelled; no changes applied.");
                return Ok(());
            }
            UninstallWizardOutcome::Completed { plan: uplan } => {
                let wizard_plan = wizard_plan_to_remove_plan(uplan, plan.preserved);
                return apply_wizard_plan(repo_root, tool, json, force, wizard_plan);
            }
        }
    }

    if json {
        print!("{}", serde_json::to_string_pretty(&plan)?);
        println!();
    } else {
        render_plan_table(&plan, tool.is_some());
    }

    if !apply {
        if !json && !plan.is_empty() {
            println!(
                "\nDry run. Run `synrepo remove{}{} --apply` to execute these actions.",
                tool.map(|t| format!(" {}", t.canonical_name()))
                    .unwrap_or_default(),
                if keep_synrepo_dir {
                    " --keep-synrepo-dir"
                } else {
                    ""
                }
            );
        }
        return Ok(());
    }

    if plan.is_empty() {
        if !json {
            println!("\nNothing to remove.");
        }
        return Ok(());
    }

    let synrepo_dir = Config::synrepo_dir(repo_root);
    let has_synrepo_action = plan
        .actions
        .iter()
        .any(|a| matches!(a, RemoveAction::DeleteSynrepoDir));
    guard_watch_daemon(
        &synrepo_dir,
        has_synrepo_action,
        force,
        /*wizard=*/ false,
    )?;

    // Bulk removal prompts before deleting `.synrepo/` unless `--force` or
    // `--keep-synrepo-dir` was passed. Per-tool removal never includes the
    // `DeleteSynrepoDir` action to begin with.
    let mut plan_to_apply = plan;
    if has_synrepo_action && !force && !prompt_delete_synrepo_dir(&synrepo_dir) {
        plan_to_apply
            .actions
            .retain(|a| !matches!(a, RemoveAction::DeleteSynrepoDir));
        if !json {
            println!(
                "  Keeping {} (you can delete it manually later).",
                synrepo_dir.display()
            );
        }
    }

    finalize_remove(repo_root, tool, &plan_to_apply, json)
}

/// Apply `plan`, render the summary, and drop the appropriate registry rows.
/// Shared by the plain-text/JSON path and the wizard path so the post-apply
/// behavior cannot drift between the two surfaces.
pub(super) fn finalize_remove(
    repo_root: &Path,
    tool: Option<AgentTool>,
    plan: &RemovePlan,
    json: bool,
) -> anyhow::Result<()> {
    let summary = apply_plan(repo_root, plan)?;
    if json {
        print!("{}", serde_json::to_string_pretty(&summary)?);
        println!();
    } else {
        render_summary(&summary);
    }

    // Best-effort registry cleanup: filesystem state has already changed, so
    // a write failure here is logged and swallowed.
    if let Some(t) = tool {
        if let Err(err) = registry::record_agent_uninstall(repo_root, t.canonical_name()) {
            tracing::warn!(error = %err, "registry update skipped after per-agent remove");
        }
    } else if let Err(err) = registry::record_uninstall(repo_root) {
        tracing::warn!(error = %err, "registry update skipped after bulk remove");
    }
    Ok(())
}

/// Refuse bulk `.synrepo/` deletion while a watch daemon is running. `force`
/// downgrades the refusal to a warning. `wizard` only changes the retry hint.
pub(super) fn guard_watch_daemon(
    synrepo_dir: &Path,
    has_synrepo_action: bool,
    force: bool,
    wizard: bool,
) -> anyhow::Result<()> {
    if !has_synrepo_action {
        return Ok(());
    }
    if !matches!(
        watch_service_status(synrepo_dir),
        WatchServiceStatus::Running(_) | WatchServiceStatus::Starting
    ) {
        return Ok(());
    }
    if !force {
        let retry_hint = if wizard {
            "re-run with `--force`"
        } else {
            "pass `--force`"
        };
        anyhow::bail!(
            "remove blocked: a watch daemon is running against {}. \
             Run `synrepo watch stop` and retry, or {retry_hint} to \
             override (you still need to stop the daemon yourself).",
            synrepo_dir.display()
        );
    }
    eprintln!(
        "warning: a watch daemon is still running against {}. \
         .synrepo/ deletion will proceed but the daemon may still hold file handles; \
         stop it with `synrepo watch stop` for a clean teardown.",
        synrepo_dir.display()
    );
    Ok(())
}

fn render_plan_table(plan: &RemovePlan, per_agent: bool) {
    if plan.is_empty() {
        if per_agent {
            println!("Nothing tracked for that agent. No actions needed.");
        } else {
            println!("No synrepo install artifacts found in this repo.");
        }
        return;
    }
    println!("{:<22} {:<26} Target", "Action", "Tool");
    println!("{}", "-".repeat(78));
    for action in &plan.actions {
        let (kind, tool, target) = match action {
            RemoveAction::DeleteShim { tool, path } => {
                ("delete-shim", tool.as_str(), path.display().to_string())
            }
            RemoveAction::StripMcpEntry { tool, path } => {
                ("strip-mcp-entry", tool.as_str(), path.display().to_string())
            }
            RemoveAction::RemoveGitignoreLine { entry } => ("gitignore-line", "-", entry.clone()),
            RemoveAction::DeleteSynrepoDir => ("delete-synrepo-dir", "-", ".synrepo/".to_string()),
        };
        println!("{kind:<22} {tool:<26} {target}");
    }
    if !plan.preserved.is_empty() {
        println!("\nPreserved (never removed):");
        for p in &plan.preserved {
            println!("  {}", p.display());
        }
    }
}

pub(super) fn render_summary(summary: &ApplySummary) {
    println!();
    for item in &summary.applied {
        let label = match &item.action {
            RemoveAction::DeleteShim { tool, path } => {
                // Registry strings are canonical-form (matches clap's
                // kebab-case value-enum form; pinned by the
                // `canonical_name_matches_clap_value_enum_form` test). Fall
                // back to "instructions" for tools a future binary knows
                // that we do not.
                let label = <AgentTool as clap::ValueEnum>::from_str(tool, false)
                    .ok()
                    .map(AgentTool::artifact_label)
                    .unwrap_or("instructions");
                format!("deleted {tool} {label} ({})", path.display())
            }
            RemoveAction::StripMcpEntry { path, .. } => {
                format!("stripped synrepo entry from {}", path.display())
            }
            RemoveAction::RemoveGitignoreLine { entry } => {
                format!("removed `{entry}` from root .gitignore")
            }
            RemoveAction::DeleteSynrepoDir => "deleted .synrepo/".to_string(),
        };
        if item.succeeded {
            println!("  ok: {label}");
        } else {
            println!(
                "  FAILED: {label} ({})",
                item.error.as_deref().unwrap_or("unknown error")
            );
        }
    }
    println!("Remove complete.");
}

/// TTY-interactive yes/no for `.synrepo/` deletion. Returns `true` on "y",
/// `false` on "n" or when stdin is not a TTY.
fn prompt_delete_synrepo_dir(synrepo_dir: &Path) -> bool {
    use std::io::{self, BufRead, Write};

    if !synrepo::tui::stdout_is_tty() {
        // Non-interactive: keep the directory by default. `--force` is the
        // explicit opt-in for "yes without asking".
        return false;
    }
    print!(
        "Delete {} and all cached graph/overlay/index data? [y/N] ",
        synrepo_dir.display()
    );
    io::stdout().flush().ok();
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Every [`AgentTool`] variant in declaration order. Sourced from clap's
/// `ValueEnum` derive so adding a new variant flows here automatically.
pub(super) fn all_agent_tools() -> &'static [AgentTool] {
    use clap::ValueEnum;
    AgentTool::value_variants()
}

#[cfg(test)]
mod tests;
