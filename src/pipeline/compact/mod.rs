//! Compaction operations for `.synrepo/` runtime stores.
//!
//! This module provides policy-driven compaction for overlay, state, and index
//! stores to reclaim disk space. It extends the maintenance module with
//! retention-based cleanup.

pub mod ops;

pub use ops::{load_last_compaction_timestamp, rotate_repair_log, wal_checkpoint};

use std::path::Path;

use time::OffsetDateTime;

use crate::config::Config;
use crate::overlay::OverlayStore;
use crate::pipeline::writer::{acquire_write_admission, map_lock_error};

/// Plan compaction by querying overlay stats, repair-log age, and index freshness.
pub fn plan_compact(
    synrepo_dir: &Path,
    _config: &Config,
    _policy: crate::pipeline::maintenance::CompactPolicy,
) -> crate::Result<crate::pipeline::maintenance::CompactPlan> {
    use crate::pipeline::maintenance::{CompactAction, CompactComponent, CompactStats};

    let mut actions = Vec::new();
    let mut stats = CompactStats {
        last_compaction_timestamp: ops::load_last_compaction_timestamp(synrepo_dir),
        ..CompactStats::default()
    };

    // Query commentary compactability.
    let overlay_dir = synrepo_dir.join("overlay");
    if overlay_dir.exists() {
        if let Ok(store) = crate::store::overlay::SqliteOverlayStore::open_existing(&overlay_dir) {
            // Get commentary count and stale count.
            if let Ok(count) = store.commentary_count() {
                // Upper bound: execution phase applies commentary_retention_days() before deletion.
                stats.compactable_commentary = count;
            }
            // Cross-link audit row count.
            if let Ok(audit_count) = store.cross_link_audit_count() {
                stats.compactable_cross_links = audit_count;
            }
        }
    }

    // Determine actions needed.
    if stats.compactable_commentary > 0 {
        actions.push((
            CompactComponent::Commentary,
            CompactAction::CompactCommentary,
        ));
    }
    if stats.compactable_cross_links > 0 {
        actions.push((
            CompactComponent::CrossLinks,
            CompactAction::CompactCrossLinks,
        ));
    }

    // Repair-log rotation.
    let log_path = synrepo_dir.join("state/repair-log.jsonl");
    if log_path.exists() {
        // Rotation is cheap and idempotent; defer the age check to rotate_repair_log().
        actions.push((CompactComponent::RepairLog, CompactAction::RotateRepairLog));
    }

    // WAL checkpoint is always beneficial.
    actions.push((CompactComponent::Wal, CompactAction::WalCheckpoint));

    // Index rebuild decision (based on config change since last compile).
    // For now, skip by default unless explicitly requested.
    // Index rebuild is handled by compatibility evaluator, not compact.

    let plan = crate::pipeline::maintenance::CompactPlan {
        actions: actions
            .into_iter()
            .map(
                |(component, action)| crate::pipeline::maintenance::ComponentCompact {
                    component,
                    action,
                    reason: String::new(), // Will be filled during execution.
                },
            )
            .collect(),
        estimated_stats: stats,
    };

    Ok(plan)
}

/// Execute the compaction plan.
pub fn execute_compact(
    synrepo_dir: &Path,
    plan: &crate::pipeline::maintenance::CompactPlan,
    policy: crate::pipeline::maintenance::CompactPolicy,
) -> crate::Result<crate::pipeline::maintenance::CompactSummary> {
    use crate::pipeline::maintenance::CompactComponent;

    // Acquire write admission for the duration of the compaction.
    let _lock = acquire_write_admission(synrepo_dir, "compact")
        .map_err(|err| map_lock_error("compact", err))?;

    let mut summary = crate::pipeline::maintenance::CompactSummary {
        compaction_timestamp: OffsetDateTime::now_utc(),
        ..Default::default()
    };

    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay_store =
        match crate::store::overlay::SqliteOverlayStore::open_existing(&overlay_dir) {
            Ok(store) => Some(store),
            Err(e) => {
                tracing::debug!(error = %e, "overlay store not available for compaction");
                None
            }
        };

    for comp in &plan.actions {
        match comp.component {
            CompactComponent::Commentary => {
                if let Some(ref mut store) = overlay_store {
                    match store.compact_commentary(&policy) {
                        Ok(count) => summary.commentary_compacted = count,
                        Err(err) => {
                            tracing::warn!(error = %err, "commentary compaction failed");
                            summary
                                .failures
                                .push(format!("commentary compaction failed: {err}"));
                        }
                    }
                }
            }
            CompactComponent::CrossLinks => {
                if let Some(ref mut store) = overlay_store {
                    match store.compact_cross_links(&policy) {
                        Ok(count) => summary.cross_links_compacted = count,
                        Err(err) => {
                            tracing::warn!(error = %err, "cross-link compaction failed");
                            summary
                                .failures
                                .push(format!("cross-link compaction failed: {err}"));
                        }
                    }
                }
            }
            CompactComponent::RepairLog => {
                let result = ops::rotate_repair_log(synrepo_dir, &policy)?;
                summary.repair_log_summarized = result.repair_log_summarized;
            }
            CompactComponent::Wal => {
                summary.wal_checkpoint_completed = ops::wal_checkpoint(synrepo_dir)?;
            }
            CompactComponent::Index => {
                // Index rebuild is handled by compatibility evaluator.
                summary.index_rebuilt = false;
            }
        }
    }

    // Record completion timestamp.
    ops::save_compaction_timestamp(synrepo_dir, summary.compaction_timestamp)?;

    Ok(summary)
}

#[cfg(test)]
mod tests;
