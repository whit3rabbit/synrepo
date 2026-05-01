//! Compaction command implementation.

use std::path::Path;

use synrepo::config::Config;
use synrepo::pipeline::maintenance::CompactPolicy;
use synrepo::pipeline::{
    compact::{execute_compact, plan_compact},
    writer::{acquire_write_admission, map_lock_error},
};

/// Print a dry-run compaction plan or execute it with `--apply`.
pub(crate) fn compact(repo_root: &Path, apply: bool, policy: CompactPolicy) -> anyhow::Result<()> {
    let config = Config::load(repo_root).map_err(|e| {
        anyhow::anyhow!("compact: not initialized, run `synrepo init --mode auto` first ({e})")
    })?;
    let synrepo_dir = Config::synrepo_dir(repo_root);

    // Plan compaction.
    let plan = plan_compact(&synrepo_dir, &config, policy)
        .map_err(|e| anyhow::anyhow!("compact: failed to plan: {e}"))?;

    // Print plan.
    println!("Compaction Plan (policy: {})", policy.as_str());
    println!("{:<20} {:<20} Reason", "Component", "Action");
    println!("{}", "-".repeat(60));

    let total_compactable =
        plan.estimated_stats.compactable_commentary + plan.estimated_stats.compactable_cross_links;
    println!(
        "  Commentary entries: {}",
        plan.estimated_stats.compactable_commentary
    );
    println!(
        "  Cross-link audit rows: {}",
        plan.estimated_stats.compactable_cross_links
    );
    println!("  Total compactable: {}", total_compactable);

    if let Some(ts) = plan.estimated_stats.last_compaction_timestamp {
        println!("  Last compaction: {}", ts);
    } else {
        println!("  Last compaction: never");
    }

    if !apply {
        println!("\nDry run. Run `synrepo compact --apply` to execute compaction.");
        return Ok(());
    }

    // Execute compaction under unified write admission.
    println!("\nExecuting compaction...");
    let _lock = acquire_write_admission(&synrepo_dir, "compact")
        .map_err(|err| map_lock_error("compact", err))?;
    let summary = execute_compact(&synrepo_dir, &plan, policy)
        .map_err(|e| anyhow::anyhow!("compact: failed to execute: {e}"))?;

    println!("\n{}", summary.render());
    if summary.has_failures() {
        return Err(anyhow::anyhow!(
            "compact: {} component(s) failed; see warnings above",
            summary.failures.len()
        ));
    }
    println!("Compaction complete.");
    Ok(())
}
