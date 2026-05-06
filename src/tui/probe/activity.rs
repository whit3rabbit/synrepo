//! Recent-activity formatting for the Live tab.

use serde_json::Value;

use crate::surface::status_snapshot::StatusSnapshot;

use super::{ActivityVm, ActivityVmEntry};

/// Build a recent-activity view model. Uses snapshot entries when the caller
/// already opted into `recent`; otherwise returns empty.
pub fn build_activity_vm(snapshot: &StatusSnapshot) -> ActivityVm {
    let Some(entries) = &snapshot.recent_activity else {
        return ActivityVm::default();
    };

    let mut out = ActivityVm::default();
    for entry in entries {
        let payload = summarize_payload(&entry.kind, &entry.payload);
        push_coalesced(
            &mut out.entries,
            ActivityVmEntry {
                timestamp: entry.timestamp.clone(),
                kind: entry.kind.clone(),
                payload,
            },
        );
    }
    out
}

fn push_coalesced(entries: &mut Vec<ActivityVmEntry>, entry: ActivityVmEntry) {
    let Some(last) = entries.last_mut() else {
        entries.push(entry);
        return;
    };
    let Some((base, count)) = coalesced_payload(&last.payload) else {
        if last.kind == entry.kind && last.payload == entry.payload {
            last.payload = format!("{} (x2)", last.payload);
            return;
        }
        entries.push(entry);
        return;
    };
    if last.kind == entry.kind && base == entry.payload {
        last.payload = format!("{base} (x{})", count + 1);
    } else {
        entries.push(entry);
    }
}

fn coalesced_payload(payload: &str) -> Option<(&str, usize)> {
    let (base, suffix) = payload.rsplit_once(" (x")?;
    let count = suffix.strip_suffix(')')?.parse().ok()?;
    Some((base, count))
}

fn summarize_payload(kind: &str, payload: &Value) -> String {
    match kind {
        "reconcile" => summarize_reconcile(payload),
        "repair" => summarize_repair(payload),
        "cross_link" => summarize_cross_link(payload),
        "overlay_refresh" => summarize_overlay_refresh(payload),
        "hotspot" => summarize_hotspot(payload),
        _ => payload.to_string(),
    }
}

fn summarize_reconcile(payload: &Value) -> String {
    let outcome = payload
        .get("outcome")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let files = payload.get("files_discovered").and_then(Value::as_u64);
    let symbols = payload.get("symbols_extracted").and_then(Value::as_u64);
    let events = payload.get("triggering_events").and_then(Value::as_u64);
    match (files, symbols, events) {
        (Some(files), Some(symbols), Some(events)) => {
            format!("{outcome}: {files} files, {symbols} symbols, {events} events")
        }
        _ => outcome.to_string(),
    }
}

fn summarize_repair(payload: &Value) -> String {
    let outcome = payload
        .get("outcome")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let actions: Vec<&str> = payload
        .get("actions_taken")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let action_summary = match actions.as_slice() {
        [] => "no actions".to_string(),
        [one] => (*one).to_string(),
        many => format!("{} actions", many.len()),
    };
    format!("{outcome}: {action_summary}")
}

fn summarize_cross_link(payload: &Value) -> String {
    let event = payload
        .get("event_kind")
        .and_then(Value::as_str)
        .unwrap_or("event");
    let kind = payload
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("link");
    format!("{event}: {kind}")
}

fn summarize_overlay_refresh(payload: &Value) -> String {
    let node = payload
        .get("node_id")
        .and_then(Value::as_str)
        .unwrap_or("node");
    let pass = payload
        .get("pass_id")
        .and_then(Value::as_str)
        .unwrap_or("refresh");
    format!("{pass}: {node}")
}

fn summarize_hotspot(payload: &Value) -> String {
    if payload.get("state").and_then(Value::as_str) == Some("unavailable") {
        return "unavailable".to_string();
    }
    let path = payload
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("path");
    let touches = payload.get("touches").and_then(Value::as_u64).unwrap_or(0);
    format!("{path}: {touches} touches")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::recent_activity::ActivityEntry;
    use crate::surface::status_snapshot::{
        CommentaryCoverage, ExportState, ExportStatus, GraphSnapshotStatus, RepairAuditState,
        StatusSnapshot,
    };
    use std::path::PathBuf;

    fn snapshot(entries: Vec<ActivityEntry>) -> StatusSnapshot {
        StatusSnapshot {
            initialized: true,
            config: None,
            diagnostics: None,
            graph_stats: None,
            graph_snapshot: GraphSnapshotStatus {
                epoch: 0,
                age_ms: 0,
                size_bytes: 0,
                file_count: 0,
                symbol_count: 0,
                edge_count: 0,
            },
            export_freshness: "current".to_string(),
            export_status: ExportStatus {
                state: ExportState::Current,
                display: "current".to_string(),
                export_dir: "synrepo-context".to_string(),
                format: Some("markdown".to_string()),
                budget: Some("normal".to_string()),
            },
            overlay_cost_summary: "0".to_string(),
            commentary_coverage: CommentaryCoverage {
                total: None,
                fresh: None,
                estimated_fresh: None,
                estimated_stale_ratio: None,
                estimate_confidence: None,
                display: "unavailable (test fixture)".to_string(),
            },
            agent_note_counts: None,
            explain_provider: None,
            explain_totals: None,
            context_metrics: None,
            last_compaction: None,
            repair_audit: RepairAuditState::Ok,
            recent_activity: Some(entries),
            synrepo_dir: PathBuf::from("/tmp/probe-activity-test"),
        }
    }

    fn repair_entry(at: &str) -> ActivityEntry {
        ActivityEntry {
            kind: "repair".to_string(),
            timestamp: at.to_string(),
            payload: serde_json::json!({
                "outcome": "completed",
                "actions_taken": [
                    "regenerated export directory (format=markdown, budget=normal)"
                ],
            }),
        }
    }

    #[test]
    fn repair_activity_is_summarized_and_coalesced() {
        let vm = build_activity_vm(&snapshot(vec![
            repair_entry("2026-04-25T00:00:02Z"),
            repair_entry("2026-04-25T00:00:01Z"),
        ]));

        assert_eq!(vm.entries.len(), 1);
        assert_eq!(vm.entries[0].kind, "repair");
        assert_eq!(
            vm.entries[0].payload,
            "completed: regenerated export directory (format=markdown, budget=normal) (x2)"
        );
    }

    #[test]
    fn reconcile_activity_is_summarized() {
        let vm = build_activity_vm(&snapshot(vec![ActivityEntry {
            kind: "reconcile".to_string(),
            timestamp: "2026-04-25T00:00:00Z".to_string(),
            payload: serde_json::json!({
                "outcome": "completed",
                "files_discovered": 904,
                "symbols_extracted": 12,
                "triggering_events": 3,
            }),
        }]));

        assert_eq!(
            vm.entries[0].payload,
            "completed: 904 files, 12 symbols, 3 events"
        );
    }
}
