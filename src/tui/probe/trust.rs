use crate::overlay::AgentNoteCounts;
use crate::surface::status_snapshot::StatusSnapshot;

use super::{Severity, TrustRow, TrustVm};

const SAMPLE_CAP: u64 = 5;

/// Build the trust-focused dashboard view model from a shared status snapshot.
pub fn build_trust_vm(snapshot: &StatusSnapshot) -> TrustVm {
    if !snapshot.initialized {
        return TrustVm {
            context_rows: vec![no_data_row("context", "repo not initialized")],
            overlay_rows: vec![no_data_row("agent notes", "repo not initialized")],
            change_rows: vec![no_data_row("current change", "repo not initialized")],
            degraded_rows: Vec::new(),
        };
    }

    let context_rows = context_rows(snapshot);
    let overlay_rows = overlay_rows(snapshot);
    let change_rows = change_rows(snapshot);
    let degraded_rows = degraded_rows(&context_rows, &overlay_rows);

    TrustVm {
        context_rows,
        overlay_rows,
        change_rows,
        degraded_rows,
    }
}

fn context_rows(snapshot: &StatusSnapshot) -> Vec<TrustRow> {
    let Some(metrics) = &snapshot.context_metrics else {
        return vec![no_data_row(
            "context metrics",
            "no card traffic recorded yet",
        )];
    };
    let served = metrics.cards_served_total.max(1);
    let stale_severity = if metrics.stale_responses_total > 0 {
        Severity::Stale
    } else {
        Severity::Healthy
    };
    let trunc_severity = if metrics.truncation_applied_total > 0 {
        Severity::Stale
    } else {
        Severity::Healthy
    };
    vec![
        TrustRow {
            label: "cards served".to_string(),
            value: metrics.cards_served_total.to_string(),
            hint: Some("source: status context metrics".to_string()),
            amount: Some(metrics.cards_served_total.min(SAMPLE_CAP)),
            total: Some(SAMPLE_CAP),
            severity: Severity::Healthy,
        },
        TrustRow {
            label: "avg card tokens".to_string(),
            value: format!("{:.1}", metrics.card_tokens_avg()),
            hint: Some("bounded response size".to_string()),
            amount: Some(metrics.card_tokens_avg().round().max(0.0) as u64),
            total: Some(4_000),
            severity: Severity::Healthy,
        },
        TrustRow {
            label: "tokens avoided".to_string(),
            value: format!("{} est.", metrics.estimated_tokens_saved_total),
            hint: Some("vs raw-file reads".to_string()),
            amount: Some(metrics.estimated_tokens_saved_total.min(20_000)),
            total: Some(20_000),
            severity: Severity::Healthy,
        },
        TrustRow {
            label: "stale responses".to_string(),
            value: metrics.stale_responses_total.to_string(),
            hint: remediation_hint(metrics.stale_responses_total, "run `synrepo check`"),
            amount: Some(metrics.stale_responses_total),
            total: Some(served),
            severity: stale_severity,
        },
        TrustRow {
            label: "truncated".to_string(),
            value: metrics.truncation_applied_total.to_string(),
            hint: remediation_hint(
                metrics.truncation_applied_total,
                "escalate budget only when needed",
            ),
            amount: Some(metrics.truncation_applied_total),
            total: Some(served),
            severity: trunc_severity,
        },
        TrustRow {
            label: "escalations".to_string(),
            value: budget_escalations(metrics),
            hint: Some("normal/deep budget usage".to_string()),
            amount: Some(budget_escalation_count(metrics)),
            total: Some(served),
            severity: Severity::Healthy,
        },
    ]
}

fn overlay_rows(snapshot: &StatusSnapshot) -> Vec<TrustRow> {
    let Some(counts) = snapshot.agent_note_counts else {
        return vec![no_data_row("agent notes", "overlay note data unavailable")];
    };
    let total = counts.total().max(1) as u64;
    note_rows(counts, total)
}

fn note_rows(counts: AgentNoteCounts, total: u64) -> Vec<TrustRow> {
    vec![
        note_row("active", counts.active, total, Severity::Healthy),
        note_row(
            "stale",
            counts.stale,
            total,
            degraded_note_severity(counts.stale),
        ),
        note_row(
            "unverified",
            counts.unverified,
            total,
            if counts.unverified > 0 {
                Severity::Stale
            } else {
                Severity::Healthy
            },
        ),
        note_row("superseded", counts.superseded, total, Severity::Healthy),
        note_row("forgotten", counts.forgotten, total, Severity::Healthy),
        note_row(
            "invalid",
            counts.invalid,
            total,
            degraded_note_severity(counts.invalid),
        ),
    ]
}

fn change_rows(snapshot: &StatusSnapshot) -> Vec<TrustRow> {
    let Some(metrics) = &snapshot.context_metrics else {
        return vec![no_data_row(
            "current change",
            "run changed-context MCP or CLI surfaces",
        )];
    };
    vec![
        TrustRow {
            label: "changed files".to_string(),
            value: bounded_count(metrics.changed_files_total),
            hint: Some(
                recent_activity_hint(snapshot)
                    .unwrap_or_else(|| "observed by changed-context surface".to_string()),
            ),
            amount: Some(metrics.changed_files_total.min(SAMPLE_CAP)),
            total: Some(SAMPLE_CAP),
            severity: Severity::Healthy,
        },
        TrustRow {
            label: "affected symbols".to_string(),
            value: "unavailable".to_string(),
            hint: Some("not present in shared snapshot yet".to_string()),
            amount: None,
            total: None,
            severity: Severity::Stale,
        },
        TrustRow {
            label: "linked tests".to_string(),
            value: bounded_count(metrics.test_surface_hits_total),
            hint: Some("test-surface hits".to_string()),
            amount: Some(metrics.test_surface_hits_total.min(SAMPLE_CAP)),
            total: Some(SAMPLE_CAP),
            severity: Severity::Healthy,
        },
        TrustRow {
            label: "open risks".to_string(),
            value: "unavailable".to_string(),
            hint: Some("run `synrepo check` or risk MCP surface".to_string()),
            amount: None,
            total: None,
            severity: Severity::Stale,
        },
    ]
}

fn degraded_rows(context: &[TrustRow], overlay: &[TrustRow]) -> Vec<TrustRow> {
    context
        .iter()
        .chain(overlay.iter())
        .filter(|row| row.severity != Severity::Healthy)
        .map(|row| TrustRow {
            label: row.label.clone(),
            value: row.value.clone(),
            hint: row
                .hint
                .clone()
                .or_else(|| Some("review trust signal".to_string())),
            amount: row.amount,
            total: row.total,
            severity: row.severity,
        })
        .collect()
}

fn note_row(label: &str, count: usize, total: u64, severity: Severity) -> TrustRow {
    TrustRow {
        label: label.to_string(),
        value: count.to_string(),
        hint: note_hint(label, count),
        amount: Some(count as u64),
        total: Some(total),
        severity,
    }
}

fn note_hint(label: &str, count: usize) -> Option<String> {
    match (label, count) {
        ("stale", n) if n > 0 => Some("run `synrepo sync` or verify notes".to_string()),
        ("invalid", n) if n > 0 => Some("audit, supersede, or forget invalid notes".to_string()),
        ("unverified", n) if n > 0 => Some("verify advisory evidence".to_string()),
        _ => Some("source: advisory overlay".to_string()),
    }
}

fn degraded_note_severity(count: usize) -> Severity {
    if count > 0 {
        Severity::Stale
    } else {
        Severity::Healthy
    }
}

fn budget_escalation_count(metrics: &crate::pipeline::context_metrics::ContextMetrics) -> u64 {
    metrics
        .budget_tier_usage
        .get("normal")
        .copied()
        .unwrap_or(0)
        + metrics.budget_tier_usage.get("deep").copied().unwrap_or(0)
}

fn budget_escalations(metrics: &crate::pipeline::context_metrics::ContextMetrics) -> String {
    let normal = metrics
        .budget_tier_usage
        .get("normal")
        .copied()
        .unwrap_or(0);
    let deep = metrics.budget_tier_usage.get("deep").copied().unwrap_or(0);
    format!("{} normal, {} deep", normal, deep)
}

fn recent_activity_hint(snapshot: &StatusSnapshot) -> Option<String> {
    let entries = snapshot.recent_activity.as_ref()?;
    entries.iter().take(SAMPLE_CAP as usize).find_map(|entry| {
        if entry.kind == "hotspot" {
            entry
                .payload
                .get("path")
                .and_then(|path| path.as_str())
                .map(|path| format!("recent hotspot: {path}"))
        } else if entry.kind == "repair" {
            Some("recent repair activity available".to_string())
        } else {
            None
        }
    })
}

fn bounded_count(count: u64) -> String {
    if count > SAMPLE_CAP {
        format!("{} sampled", SAMPLE_CAP)
    } else {
        count.to_string()
    }
}

fn remediation_hint(count: u64, hint: &str) -> Option<String> {
    if count > 0 {
        Some(hint.to_string())
    } else {
        Some("no degraded samples recorded".to_string())
    }
}

fn no_data_row(label: &str, reason: &str) -> TrustRow {
    TrustRow {
        label: label.to_string(),
        value: "no data".to_string(),
        hint: Some(reason.to_string()),
        amount: None,
        total: None,
        severity: Severity::Stale,
    }
}
