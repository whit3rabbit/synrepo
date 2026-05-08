//! Maintenance hooks for `.synrepo/` runtime stores.
//!
//! Maintenance behavior is derived from the existing `storage-compatibility`
//! contract rather than defining its own retention rules. This ensures that
//! cleanup and compaction decisions remain consistent with the store class and
//! compatibility policy already defined in `store::compatibility`.
//!
//! ## Design
//!
//! `plan_maintenance` queries the current compatibility state and maps each
//! store's `CompatAction` to a `MaintenanceAction`. Blocking actions (`Block`)
//! are surfaced as informational guidance but never executed automatically;
//! they require explicit operator intervention.
//!
//! `execute_maintenance` applies the plan: clearing or rebuilding stores
//! whose compatibility actions indicate stale or incompatible contents.
//!
//! ## Compact
//!
//! `plan_compact` queries overlay stats, repair-log age, and index freshness
//! to build a `CompactPlan`. `execute_compact` runs all retention actions
//! in sequence: commentary compaction, cross-link compaction, repair-log
//! rotation, WAL checkpoint, and optional index rebuild.

pub mod types;

mod compatibility_apply;

pub use compatibility_apply::*;
pub use types::*;

use std::path::Path;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::{
    config::Config,
    store::compatibility::{self, clear_store_contents, CompatAction},
};

/// Compute a retention cutoff timestamp (formatted as RFC3339 string).
/// Used by overlay compaction to compute deletion boundaries.
pub fn retention_cutoff(retention_days: u32) -> crate::Result<String> {
    let cutoff = OffsetDateTime::now_utc() - time::Duration::days(retention_days as i64);
    cutoff
        .format(&Rfc3339)
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("format cutoff: {e}")))
}

/// Plan maintenance actions by consulting the current compatibility state.
///
/// Derives maintenance needs from the `storage-compatibility` contract so
/// that cleanup decisions stay consistent with the declared store classes
/// and compatibility policies already established in `storage-compatibility-v1`.
pub fn plan_maintenance(synrepo_dir: &Path, config: &Config) -> crate::Result<MaintenancePlan> {
    let runtime_exists = synrepo_dir.exists();
    let report = compatibility::evaluate_runtime(synrepo_dir, runtime_exists, config)?;

    let actions = report
        .entries
        .iter()
        .map(|entry| StoreMaintenance {
            store_id: entry.store_id,
            action: compat_action_to_maintenance(entry.action),
            reason: entry.reason.clone(),
        })
        .collect();

    Ok(MaintenancePlan { actions })
}

/// Execute the maintenance plan, applying non-blocking actions to disk.
///
/// Blocking actions (`Block`) are not executed here and do not contribute to
/// `stores_cleared`; they require explicit operator intervention such as
/// removing or re-initializing the affected store.
pub fn execute_maintenance(
    synrepo_dir: &Path,
    plan: &MaintenancePlan,
) -> crate::Result<MaintenanceSummary> {
    let mut summary = MaintenanceSummary::default();

    for store_action in plan.pending_actions() {
        clear_store_contents(synrepo_dir, store_action.store_id)?;
        summary.stores_cleared += 1;
        tracing::info!(
            store = %store_action.store_id.as_str(),
            action = %store_action.action.as_str(),
            reason = %store_action.reason,
            "maintenance action applied"
        );
    }
    summary.stores_skipped = plan.actions.len() - summary.stores_cleared;

    Ok(summary)
}

/// Map a compatibility action to the corresponding maintenance action.
fn compat_action_to_maintenance(action: CompatAction) -> MaintenanceAction {
    match action {
        CompatAction::Continue => MaintenanceAction::Skip,
        CompatAction::Rebuild | CompatAction::ClearAndRecreate => MaintenanceAction::Rebuild,
        CompatAction::Invalidate => MaintenanceAction::Clear,
        // Blocking actions require explicit operator intervention and are
        // intentionally not executed by the maintenance runner.
        CompatAction::Block => MaintenanceAction::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::compatibility::{ensure_runtime_layout, write_runtime_snapshot, StoreId};
    use tempfile::tempdir;

    fn init_runtime(synrepo_dir: &Path) {
        ensure_runtime_layout(synrepo_dir).unwrap();
        write_runtime_snapshot(synrepo_dir, &Config::default()).unwrap();
    }

    #[test]
    fn plan_has_no_work_on_freshly_initialized_runtime() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        init_runtime(&synrepo_dir);

        let plan = plan_maintenance(&synrepo_dir, &Config::default()).unwrap();
        assert!(
            !plan.has_work(),
            "fresh runtime with current snapshot should need no maintenance"
        );
    }

    #[test]
    fn plan_schedules_rebuild_for_stale_index() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        init_runtime(&synrepo_dir);

        // Materialize the index so the compatibility check sees it.
        std::fs::write(synrepo_dir.join("index/manifest.json"), "{}").unwrap();

        // Change an index-sensitive config field to trigger a Rebuild action.
        let updated = Config {
            roots: vec!["src".to_string()],
            ..Config::default()
        };
        let plan = plan_maintenance(&synrepo_dir, &updated).unwrap();

        let index_action = plan
            .actions
            .iter()
            .find(|a| a.store_id == StoreId::Index)
            .expect("index store must be in plan");
        assert_eq!(index_action.action, MaintenanceAction::Rebuild);
        assert!(plan.has_work());
    }

    #[test]
    fn execute_maintenance_clears_stale_index_contents() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        init_runtime(&synrepo_dir);

        let index_file = synrepo_dir.join("index/manifest.json");
        std::fs::write(&index_file, "{}").unwrap();
        assert!(
            index_file.exists(),
            "index file must exist before maintenance"
        );

        let updated = Config {
            roots: vec!["src".to_string()],
            ..Config::default()
        };
        let plan = plan_maintenance(&synrepo_dir, &updated).unwrap();
        assert!(plan.has_work());

        let summary = execute_maintenance(&synrepo_dir, &plan).unwrap();
        assert!(summary.stores_cleared >= 1);

        let remaining: Vec<_> = std::fs::read_dir(synrepo_dir.join("index"))
            .unwrap()
            .collect();
        assert!(
            remaining.is_empty(),
            "index must be empty after maintenance"
        );
    }

    #[test]
    fn execute_maintenance_skips_healthy_stores() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        init_runtime(&synrepo_dir);

        let plan = plan_maintenance(&synrepo_dir, &Config::default()).unwrap();
        let summary = execute_maintenance(&synrepo_dir, &plan).unwrap();

        assert_eq!(summary.stores_cleared, 0);
        // All known stores are covered.
        assert_eq!(
            summary.stores_skipped,
            StoreId::ALL.len(),
            "all stores should be skipped when none need maintenance"
        );
    }

    #[test]
    fn maintenance_summary_render_reflects_cleared_count() {
        let summary = MaintenanceSummary {
            stores_cleared: 2,
            stores_skipped: 4,
        };
        let rendered = summary.render();
        assert!(rendered.contains("Cleared 2"), "must mention cleared count");
        assert!(rendered.contains('4'), "must mention skipped count");
    }

    #[test]
    fn maintenance_summary_render_for_no_work_done() {
        let summary = MaintenanceSummary {
            stores_cleared: 0,
            stores_skipped: 6,
        };
        assert!(summary.render().contains("No maintenance needed"));
    }

    #[test]
    fn execute_compact_preserves_graph_row_counts() {
        use std::fs;

        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        let graph_dir = synrepo_dir.join("graph");
        let overlay_dir = synrepo_dir.join("overlay");
        let state_dir = synrepo_dir.join("state");

        fs::create_dir_all(&graph_dir).unwrap();
        fs::create_dir_all(&overlay_dir).unwrap();
        fs::create_dir_all(&state_dir).unwrap();

        let conn = rusqlite::Connection::open(graph_dir.join("nodes.db")).unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (id TEXT PRIMARY KEY, path TEXT, content_hash TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS symbols (id TEXT PRIMARY KEY, file_id TEXT, qualified_name TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS edges (id TEXT PRIMARY KEY, from_node TEXT, to_node TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files (id, path) VALUES ('file_1', 'src/lib.rs')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files (id, path) VALUES ('file_2', 'src/main.rs')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols (id, file_id, qualified_name) VALUES ('sym_1', 'file_1', 'main')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO edges (id, from_node, to_node) VALUES ('edge_1', 'file_1', 'sym_1')",
            [],
        )
        .unwrap();
        drop(conn);

        use crate::overlay::OverlayStore;
        use crate::store::overlay::SqliteOverlayStore;
        let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
        let old_ts = OffsetDateTime::now_utc() - time::Duration::days(60);
        overlay
            .insert_commentary(crate::overlay::CommentaryEntry {
                node_id: crate::core::ids::NodeId::Symbol(crate::core::ids::SymbolNodeId(1)),
                text: "old".to_string(),
                provenance: crate::overlay::CommentaryProvenance {
                    source_content_hash: "h".to_string(),
                    pass_id: "test".to_string(),
                    model_identity: "test".to_string(),
                    generated_at: old_ts,
                },
            })
            .unwrap();
        overlay.commit().unwrap();

        let conn = rusqlite::Connection::open(graph_dir.join("nodes.db")).unwrap();
        let pre_files: i64 = conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap();
        let pre_symbols: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
            .unwrap();
        let pre_edges: i64 = conn
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
            .unwrap();
        drop(conn);

        let config = Config::default();
        let plan =
            crate::pipeline::compact::plan_compact(&synrepo_dir, &config, CompactPolicy::Default)
                .unwrap();
        let _summary =
            crate::pipeline::compact::execute_compact(&synrepo_dir, &plan, CompactPolicy::Default)
                .unwrap();

        let conn = rusqlite::Connection::open(graph_dir.join("nodes.db")).unwrap();
        let post_files: i64 = conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap();
        let post_symbols: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
            .unwrap();
        let post_edges: i64 = conn
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
            .unwrap();
        drop(conn);

        assert_eq!(pre_files, post_files, "files rows must be preserved");
        assert_eq!(pre_symbols, post_symbols, "symbols rows must be preserved");
        assert_eq!(pre_edges, post_edges, "edges rows must be preserved");
    }
}
