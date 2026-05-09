use std::path::Path;

use serde_json::json;

use crate::{
    bootstrap::runtime_probe::probe,
    pipeline::{
        diagnostics::{ReconcileHealth, ReconcileStaleness},
        watch::WatchServiceStatus,
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    surface::{
        readiness::{ReadinessMatrix, ReadinessRow},
        status_snapshot::{build_status_snapshot, StatusOptions, StatusSnapshot},
    },
};

use super::{helpers::render_result, SynrepoState};

pub fn handle_readiness(state: &SynrepoState, overlay_writes: bool, source_edits: bool) -> String {
    render_result(build_readiness(state, overlay_writes, source_edits))
}

fn build_readiness(
    state: &SynrepoState,
    overlay_writes: bool,
    source_edits: bool,
) -> anyhow::Result<serde_json::Value> {
    let snapshot = build_status_snapshot(
        &state.repo_root,
        StatusOptions {
            recent: false,
            full: false,
        },
    );
    let report = probe(&state.repo_root);
    let config = snapshot
        .config
        .as_ref()
        .cloned()
        .unwrap_or_else(|| state.config.clone());
    let matrix = ReadinessMatrix::build(&state.repo_root, &report, &snapshot, &config);

    let graph_db = SqliteGraphStore::db_path(&snapshot.synrepo_dir.join("graph"));
    let overlay_db = SqliteOverlayStore::db_path(&snapshot.synrepo_dir.join("overlay"));
    let index_manifest = snapshot.synrepo_dir.join("index").join("manifest.json");
    let diagnostics = snapshot.diagnostics.as_ref();

    Ok(json!({
        "ok": true,
        "schema_version": 1,
        "repo_root": state.repo_root,
        "graph": graph_status(&snapshot, &graph_db),
        "overlay": overlay_status(&snapshot),
        "index": index_status(index_manifest.exists(), diagnostics.map(|diag| &diag.reconcile_health)),
        "watch": diagnostics
            .map(|diag| watch_status(&diag.watch_status))
            .unwrap_or("error"),
        "reconcile": diagnostics
            .map(|diag| reconcile_status(&diag.reconcile_health))
            .unwrap_or("missing"),
        "edit_mode": {
            "overlay_writes": overlay_writes,
            "source_edits": source_edits,
        },
        "details": {
            "graph": {
                "db_path": graph_db,
                "materialized": graph_db.exists(),
                "stats": snapshot.graph_stats,
                "snapshot": {
                    "epoch": snapshot.graph_snapshot.epoch,
                    "age_ms": snapshot.graph_snapshot.age_ms,
                    "size_bytes": snapshot.graph_snapshot.size_bytes,
                    "file_count": snapshot.graph_snapshot.file_count,
                    "symbol_count": snapshot.graph_snapshot.symbol_count,
                    "edge_count": snapshot.graph_snapshot.edge_count,
                },
            },
            "overlay": {
                "db_path": overlay_db,
                "materialized": overlay_db.exists(),
                "commentary": {
                    "coverage": snapshot.commentary_coverage.display,
                    "total": snapshot.commentary_coverage.total,
                    "fresh": snapshot.commentary_coverage.fresh,
                    "estimated_fresh": snapshot.commentary_coverage.estimated_fresh,
                    "estimated_stale_ratio": snapshot.commentary_coverage.estimated_stale_ratio,
                    "estimate_confidence": snapshot.commentary_coverage.estimate_confidence,
                },
                "agent_notes": snapshot.agent_note_counts,
            },
            "index": {
                "manifest_path": index_manifest,
                "manifest_exists": index_manifest.exists(),
            },
            "watch": diagnostics.map(|diag| watch_detail(&diag.watch_status)),
            "reconcile": diagnostics.map(|diag| {
                json!({
                    "health": format!("{:?}", diag.reconcile_health),
                    "last": diag.last_reconcile,
                })
            }),
            "capabilities": readiness_rows(&matrix),
        },
    }))
}

fn graph_status(snapshot: &StatusSnapshot, db_path: &Path) -> &'static str {
    if snapshot.graph_stats.is_some() {
        "ready"
    } else if db_path.exists() {
        "error"
    } else {
        "missing"
    }
}

fn overlay_status(snapshot: &StatusSnapshot) -> &'static str {
    snapshot.overlay_state.as_str()
}

fn index_status(manifest_exists: bool, reconcile: Option<&ReconcileHealth>) -> &'static str {
    if !manifest_exists {
        return "missing";
    }
    match reconcile {
        Some(ReconcileHealth::Current) => "ready",
        Some(
            ReconcileHealth::Stale(_)
            | ReconcileHealth::WatchStalled { .. }
            | ReconcileHealth::Unknown
            | ReconcileHealth::Corrupt(_),
        ) => "stale",
        None => "missing",
    }
}

fn watch_status(status: &WatchServiceStatus) -> &'static str {
    match status {
        WatchServiceStatus::Running(_) => "active",
        WatchServiceStatus::Starting => "starting",
        WatchServiceStatus::Inactive => "inactive",
        WatchServiceStatus::Stale(_) => "stale",
        WatchServiceStatus::Corrupt(_) => "error",
    }
}

fn reconcile_status(status: &ReconcileHealth) -> &'static str {
    match status {
        ReconcileHealth::Current => "fresh",
        ReconcileHealth::Stale(ReconcileStaleness::Age { .. })
        | ReconcileHealth::Stale(ReconcileStaleness::Outcome(_))
        | ReconcileHealth::WatchStalled { .. } => "stale",
        ReconcileHealth::Unknown => "missing",
        ReconcileHealth::Corrupt(_) => "error",
    }
}

fn watch_detail(status: &WatchServiceStatus) -> serde_json::Value {
    match status {
        WatchServiceStatus::Running(state) => json!({
            "status": "active",
            "pid": state.pid,
            "mode": state.mode,
            "started_at": state.started_at,
            "last_event_at": state.last_event_at,
            "last_reconcile_at": state.last_reconcile_at,
            "last_reconcile_outcome": state.last_reconcile_outcome,
            "auto_sync_enabled": state.auto_sync_enabled,
            "auto_sync_running": state.auto_sync_running,
            "auto_sync_paused": state.auto_sync_paused,
        }),
        WatchServiceStatus::Starting => json!({ "status": "starting" }),
        WatchServiceStatus::Inactive => json!({ "status": "inactive" }),
        WatchServiceStatus::Stale(state) => json!({
            "status": "stale",
            "last_owner": state,
        }),
        WatchServiceStatus::Corrupt(error) => json!({
            "status": "error",
            "error": error,
        }),
    }
}

fn readiness_rows(matrix: &ReadinessMatrix) -> Vec<serde_json::Value> {
    matrix.rows.iter().map(readiness_row).collect::<Vec<_>>()
}

fn readiness_row(row: &ReadinessRow) -> serde_json::Value {
    json!({
        "capability": row.capability.as_str(),
        "state": row.state.as_str(),
        "severity": row.state.severity().as_str(),
        "detail": row.detail,
        "next_action": row.next_action,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::{tempdir, TempDir};

    use super::*;
    use crate::{
        bootstrap::bootstrap,
        config::{test_home, Config},
    };

    struct ReadyState {
        _home: TempDir,
        _guard: test_home::HomeEnvGuard,
        _repo: TempDir,
        state: SynrepoState,
    }

    fn ready_state() -> ReadyState {
        let home = tempdir().unwrap();
        let guard = test_home::HomeEnvGuard::redirect_to(home.path());
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn readiness() {}\n").unwrap();
        bootstrap(repo.path(), None, false).unwrap();
        let state = SynrepoState {
            config: Config::load(repo.path()).unwrap(),
            repo_root: repo.path().to_path_buf(),
        };
        ReadyState {
            _home: home,
            _guard: guard,
            _repo: repo,
            state,
        }
    }

    #[test]
    fn readiness_reports_core_status_and_default_modes() {
        let fixture = ready_state();
        let output = handle_readiness(&fixture.state, false, false);
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(value["ok"], true, "{output}");
        assert_eq!(value["graph"], "ready", "{output}");
        assert_eq!(value["overlay"], "ready_empty", "{output}");
        assert_eq!(value["index"], "ready", "{output}");
        assert_eq!(value["watch"], "inactive", "{output}");
        assert_eq!(value["reconcile"], "fresh", "{output}");
        assert_eq!(value["edit_mode"]["overlay_writes"], false, "{output}");
        assert_eq!(value["edit_mode"]["source_edits"], false, "{output}");
        assert!(value["details"]["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["capability"] == "index-freshness"));
    }

    #[test]
    fn readiness_reports_enabled_mutation_modes() {
        let fixture = ready_state();
        let output = handle_readiness(&fixture.state, true, false);
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(value["edit_mode"]["overlay_writes"], true, "{output}");
        assert_eq!(value["edit_mode"]["source_edits"], false, "{output}");
    }

    #[test]
    fn readiness_reports_missing_index_manifest() {
        let fixture = ready_state();
        let manifest = Config::synrepo_dir(&fixture.state.repo_root)
            .join("index")
            .join("manifest.json");
        fs::remove_file(&manifest).unwrap();

        let output = handle_readiness(&fixture.state, false, false);
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(value["index"], "missing", "{output}");
        assert_eq!(
            value["details"]["index"]["manifest_exists"], false,
            "{output}"
        );
    }

    #[test]
    fn readiness_reports_missing_overlay_store() {
        let fixture = ready_state();
        let overlay_db = SqliteOverlayStore::db_path(
            &Config::synrepo_dir(&fixture.state.repo_root).join("overlay"),
        );
        fs::remove_file(&overlay_db).unwrap();

        let output = handle_readiness(&fixture.state, false, false);
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(value["overlay"], "missing", "{output}");
        assert_eq!(
            value["details"]["overlay"]["materialized"], false,
            "{output}"
        );
    }

    #[test]
    fn readiness_reports_corrupt_overlay_store() {
        let fixture = ready_state();
        let overlay_db = SqliteOverlayStore::db_path(
            &Config::synrepo_dir(&fixture.state.repo_root).join("overlay"),
        );
        fs::write(&overlay_db, b"not sqlite").unwrap();

        let output = handle_readiness(&fixture.state, false, false);
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(value["overlay"], "error", "{output}");
    }
}
