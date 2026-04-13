use std::path::Path;

use synrepo::config::{Config, Mode};

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
