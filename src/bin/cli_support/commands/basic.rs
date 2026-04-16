use std::path::Path;

use synrepo::config::{Config, Mode};
use synrepo::surface::card::{Budget, CardCompiler};

use crate::cli_support::agent_shims::AgentTool;

use super::watch::ensure_watch_not_running;

/// Initialize the repository with the specified mode.
pub(crate) fn init(repo_root: &Path, requested_mode: Option<Mode>) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    ensure_watch_not_running(&synrepo_dir, "init")?;

    let report = synrepo::bootstrap::bootstrap(repo_root, requested_mode)?;
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

/// Generate a thin integration shim for the specified agent CLI.
pub(crate) fn agent_setup(
    repo_root: &Path,
    tool: AgentTool,
    force: bool,
    regen: bool,
) -> anyhow::Result<()> {
    let out_path = tool.output_path(repo_root);
    let content = tool.shim_content();

    if regen && out_path.exists() {
        let existing = std::fs::read_to_string(&out_path).unwrap_or_default();
        if existing == content {
            println!(
                "{} shim is already current: {}",
                tool.display_name(),
                out_path.display()
            );
            return Ok(());
        }
        // Different content: fall through to write.
        println!(
            "Updating {} shim (content changed): {}",
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
        println!("Wrote {} shim: {}", tool.display_name(), out_path.display());
    }
    println!("  {}", tool.include_instruction());
    Ok(())
}
