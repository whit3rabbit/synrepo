use std::path::Path;

use synrepo::config::{Config, Mode};
use synrepo::surface::card::{Budget, CardCompiler};

use crate::cli_support::agent_shims::{registry as shim_registry, AgentTool};

use super::watch::ensure_watch_not_running;

/// Initialize the repository with the specified mode.
pub(crate) fn init(
    repo_root: &Path,
    requested_mode: Option<Mode>,
    gitignore: bool,
) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    ensure_watch_not_running(&synrepo_dir, "init")?;

    let report = synrepo::bootstrap::bootstrap(repo_root, requested_mode, gitignore)?;
    print!("{}", report.render());
    Ok(())
}

/// Render the change risk output as a String (test-friendly equivalent of `change_risk`).
/// Output is identical to what `change_risk` prints, including trailing newlines.
pub(crate) fn change_risk_output(
    repo_root: &Path,
    target: &str,
    budget: Option<&str>,
    json: bool,
) -> anyhow::Result<String> {
    use synrepo::store::sqlite::SqliteGraphStore;

    let synrepo_dir = Config::synrepo_dir(repo_root);
    ensure_watch_not_running(&synrepo_dir, "change-risk")?;

    let budget = match budget {
        Some("tiny") => Budget::Tiny,
        Some("normal") => Budget::Normal,
        Some("deep") => Budget::Deep,
        Some(b) => anyhow::bail!("invalid budget: {b} (expected tiny, normal, or deep)"),
        None => Budget::Tiny,
    };

    let graph_dir = synrepo_dir.join("graph");
    let graph = SqliteGraphStore::open_existing(&graph_dir)?;
    let config = Config::load(repo_root)?;

    let compiler =
        synrepo::surface::card::compiler::GraphCardCompiler::new(Box::new(graph), Some(repo_root))
            .with_config(config);

    let node_id = compiler
        .resolve_target(target)?
        .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;

    let card = compiler.change_risk_card(node_id, budget)?;

    let mut out = String::new();
    if json {
        out.push_str(&serde_json::to_string_pretty(&card)?);
    } else {
        out.push_str(&format!(
            "Change Risk: {} ({})\n",
            card.target_name, card.target_kind
        ));
        out.push_str(&format!("  Risk level: {:?}\n", card.risk_level));
        out.push_str(&format!("  Risk score: {:.2}\n", card.risk_score));
        if !card.risk_factors.is_empty() {
            out.push_str("  Factors:\n");
            for f in &card.risk_factors {
                out.push_str(&format!(
                    "    - {}: {:.2} ({})\n",
                    f.signal, f.normalized_value, f.description
                ));
            }
        }
    }
    out.push('\n');
    Ok(out)
}

/// Output change risk assessment for a target.
pub(crate) fn change_risk(
    repo_root: &Path,
    target: &str,
    budget: Option<&str>,
    json: bool,
) -> anyhow::Result<()> {
    let out = change_risk_output(repo_root, target, budget, json)?;
    print!("{}", out);
    Ok(())
}

/// Generate the agent skill or instructions file for the specified agent CLI.
pub(crate) fn agent_setup(
    repo_root: &Path,
    tool: AgentTool,
    force: bool,
    regen: bool,
) -> anyhow::Result<()> {
    let out_path = tool.output_path(repo_root);
    let content = tool.shim_content();
    let label = tool.artifact_label();

    if regen && out_path.exists() {
        let existing = std::fs::read_to_string(&out_path).unwrap_or_default();
        if existing == content {
            println!(
                "{} {label} is already current: {}",
                tool.display_name(),
                out_path.display()
            );
            return Ok(());
        }
        // Different content: fall through to write.
        println!(
            "Updating {} {label} (content changed): {}",
            tool.display_name(),
            out_path.display()
        );
    } else if out_path.exists() && !force {
        println!(
            "synrepo agent-setup: {} already exists.",
            out_path.display()
        );
        println!("  Pass --force to overwrite, or --regen to update if stale.");
        return Ok(());
    }

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| anyhow::anyhow!("could not create {}: {error}", parent.display()))?;
    }

    std::fs::write(&out_path, content)
        .map_err(|error| anyhow::anyhow!("could not write {}: {error}", out_path.display()))?;

    if !regen {
        println!(
            "Wrote {} {label}: {}",
            tool.display_name(),
            out_path.display()
        );
    }
    println!("  {}", tool.include_instruction());

    // Shim-only: no MCP config written, so removal should not expect an
    // MCP entry for this agent in the project registry.
    shim_registry::record_install_best_effort(repo_root, tool, false, None);

    Ok(())
}

/// Install Git hooks (post-commit, post-merge, post-checkout) to trigger reconcile --fast.
pub(crate) fn install_hooks(repo_root: &Path) -> anyhow::Result<()> {
    let repo = synrepo::pipeline::git::open_repo(repo_root)
        .map_err(|e| anyhow::anyhow!("install-hooks: not a git repository: {e}"))?;

    let git_dir = repo.git_dir();
    let hooks_dir = git_dir.join("hooks");

    if !hooks_dir.exists() {
        std::fs::create_dir_all(&hooks_dir)
            .map_err(|e| anyhow::anyhow!("could not create hooks directory: {e}"))?;
    }

    let hook_script = r#"#!/bin/bash
# synrepo hook: keep the graph aligned with the working tree.
# Run reconcile in the background so it doesn't block the git command.
(synrepo reconcile --fast > /dev/null 2>&1 &)
"#;

    let hooks = ["post-commit", "post-merge", "post-checkout"];

    for hook_name in &hooks {
        let hook_path = hooks_dir.join(hook_name);
        let mut write_hook = true;

        if hook_path.exists() {
            let existing = std::fs::read_to_string(&hook_path)?;
            if existing.contains("synrepo reconcile") {
                println!("  hook already installed: {hook_name}");
                write_hook = false;
            } else {
                // Append if it's not already there
                let mut f = std::fs::OpenOptions::new().append(true).open(&hook_path)?;
                use std::io::Write;
                writeln!(f, "\n# synrepo hook\n(synrepo reconcile --fast > /dev/null 2>&1 &)")?;
                println!("  appended to existing hook: {hook_name}");
                write_hook = false;
            }
        }

        if write_hook {
            std::fs::write(&hook_path, hook_script)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&hook_path)?.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&hook_path, perms)?;
            }
            println!("  installed new hook: {hook_name}");
        }
    }

    println!("Git hooks installed successfully in {}.", hooks_dir.display());
    Ok(())
}
