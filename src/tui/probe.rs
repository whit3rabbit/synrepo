//! Compact view models over `crate::bootstrap::runtime_probe` and
//! `crate::surface::status_snapshot` for widget consumption. Kept narrow so
//! widgets don't import ratatui types into the probe modules.

use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind};
use crate::config::home_dir;
use crate::pipeline::diagnostics::{ReconcileHealth, WriterStatus};
use crate::pipeline::explain::ExplainStatus;
use crate::pipeline::watch::WatchServiceStatus;
use crate::surface::status_snapshot::StatusSnapshot;

/// Severity tag used by the dashboard to pick a color token and pane ordering.
/// `Healthy` is the baseline; `Stale` and `Blocked` escalate attention.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    /// Healthy, no action required.
    Healthy,
    /// Stale or degraded, worth noting but not blocking.
    Stale,
    /// Blocked or error — operator action required.
    Blocked,
}

/// Flattened header view model consumed by the header widget.
#[derive(Clone, Debug)]
pub struct HeaderVm {
    /// Human-readable repo path.
    pub repo_display: String,
    /// Mode label (`auto`, `curated`, `unknown`).
    pub mode_label: String,
    /// Reconcile health summary.
    pub reconcile_label: String,
    /// Reconcile severity.
    pub reconcile_severity: Severity,
    /// Watch summary.
    pub watch_label: String,
    /// Watch severity (Healthy when running, Stale when inactive, Blocked on
    /// corrupt state).
    pub watch_severity: Severity,
    /// Writer-lock summary.
    pub lock_label: String,
    /// Writer-lock severity.
    pub lock_severity: Severity,
    /// MCP readiness label ("registered", "instructions only", "absent", "n/a").
    pub mcp_label: String,
    /// MCP severity.
    pub mcp_severity: Severity,
}

/// Build a header view model from a pre-built status snapshot and the probe's
/// agent-integration signal.
pub fn build_header_vm(
    repo_display: String,
    snapshot: &StatusSnapshot,
    integration: &AgentIntegration,
) -> HeaderVm {
    let mode_label = snapshot
        .config
        .as_ref()
        .map(|c| c.mode.to_string())
        .unwrap_or_else(|| "uninitialized".to_string());

    let (reconcile_label, reconcile_severity) = match snapshot.diagnostics.as_ref() {
        None => ("uninitialized".to_string(), Severity::Stale),
        Some(d) => match &d.reconcile_health {
            ReconcileHealth::Current => ("current".to_string(), Severity::Healthy),
            ReconcileHealth::Stale(_) => ("stale".to_string(), Severity::Stale),
            ReconcileHealth::Unknown => ("unknown".to_string(), Severity::Stale),
            ReconcileHealth::Corrupt(_) => ("corrupt".to_string(), Severity::Blocked),
        },
    };

    let (watch_label, watch_severity) = match snapshot.diagnostics.as_ref() {
        None => ("uninitialized".to_string(), Severity::Stale),
        Some(d) => {
            let label = match &d.watch_status {
                WatchServiceStatus::Running(s) => format!("{} (pid {})", s.mode, s.pid),
                WatchServiceStatus::Starting => "starting".to_string(),
                WatchServiceStatus::Inactive => "inactive".to_string(),
                WatchServiceStatus::Stale(Some(s)) => format!("stale (pid {})", s.pid),
                WatchServiceStatus::Stale(None) => "stale artifacts".to_string(),
                WatchServiceStatus::Corrupt(e) => format!("corrupt ({e})"),
            };
            let sev = match &d.watch_status {
                WatchServiceStatus::Running(_) | WatchServiceStatus::Starting => Severity::Healthy,
                WatchServiceStatus::Inactive => Severity::Stale,
                WatchServiceStatus::Stale(_) => Severity::Stale,
                WatchServiceStatus::Corrupt(_) => Severity::Blocked,
            };
            (label, sev)
        }
    };

    let (lock_label, lock_severity) = match snapshot.diagnostics.as_ref() {
        None => ("n/a".to_string(), Severity::Stale),
        Some(d) => match &d.writer_status {
            WriterStatus::Free => ("free".to_string(), Severity::Healthy),
            WriterStatus::HeldBySelf => ("held by self".to_string(), Severity::Healthy),
            WriterStatus::HeldByOther { pid } => (format!("held by pid {pid}"), Severity::Stale),
            WriterStatus::Corrupt(e) => (format!("corrupt ({e})"), Severity::Blocked),
        },
    };

    let (mcp_label, mcp_severity) = match integration {
        AgentIntegration::Complete { target } => (
            format!("registered ({})", target.as_str()),
            Severity::Healthy,
        ),
        AgentIntegration::Partial { target } => (
            format!("instructions only ({})", target.as_str()),
            Severity::Stale,
        ),
        AgentIntegration::Absent => ("absent".to_string(), Severity::Stale),
    };

    HeaderVm {
        repo_display,
        mode_label,
        reconcile_label,
        reconcile_severity,
        watch_label,
        watch_severity,
        lock_label,
        lock_severity,
        mcp_label,
        mcp_severity,
    }
}

/// Flattened system-health view model.
#[derive(Clone, Debug)]
pub struct HealthVm {
    /// Rows rendered top-to-bottom.
    pub rows: Vec<HealthRow>,
}

/// One row in the system-health pane.
#[derive(Clone, Debug)]
pub struct HealthRow {
    /// Label on the left.
    pub label: String,
    /// Value on the right.
    pub value: String,
    /// Severity driving color.
    pub severity: Severity,
}

/// Build a system-health view model from a status snapshot.
pub fn build_health_vm(snapshot: &StatusSnapshot) -> HealthVm {
    let mut rows: Vec<HealthRow> = Vec::new();
    if !snapshot.initialized {
        rows.push(HealthRow {
            label: "repo".to_string(),
            value: "not initialized".to_string(),
            severity: Severity::Stale,
        });
        return HealthVm { rows };
    }

    if let Some(stats) = &snapshot.graph_stats {
        rows.push(HealthRow {
            label: "graph".to_string(),
            value: format!(
                "{} files, {} symbols, {} concepts",
                stats.file_nodes, stats.symbol_nodes, stats.concept_nodes
            ),
            severity: Severity::Healthy,
        });
    } else {
        rows.push(HealthRow {
            label: "graph".to_string(),
            value: "not materialized".to_string(),
            severity: Severity::Blocked,
        });
    }

    // Export freshness: stale if the display starts with "stale" or "absent".
    let export = &snapshot.export_freshness;
    let export_sev = if export.starts_with("current") {
        Severity::Healthy
    } else {
        Severity::Stale
    };
    rows.push(HealthRow {
        label: "export".to_string(),
        value: export.clone(),
        severity: export_sev,
    });

    rows.push(HealthRow {
        label: "commentary".to_string(),
        value: snapshot.commentary_coverage.display.clone(),
        severity: Severity::Healthy,
    });

    rows.push(HealthRow {
        label: "overlay cost".to_string(),
        value: snapshot.overlay_cost_summary.clone(),
        severity: Severity::Healthy,
    });

    if let Some(metrics) = &snapshot.context_metrics {
        rows.push(HealthRow {
            label: "context".to_string(),
            value: format!(
                "{} cards, {:.1} avg tokens",
                metrics.cards_served_total,
                metrics.card_tokens_avg()
            ),
            severity: Severity::Healthy,
        });
    }

    // Explain row: expected-off is Healthy; a detected but unused key
    // escalates to Stale so the dashboard nudges the user toward opt-in.
    if let Some(explain) = &snapshot.explain_provider {
        let (value, severity) = match &explain.status {
            ExplainStatus::Enabled => {
                let model = explain
                    .model
                    .as_deref()
                    .map(|m| format!(" ({m})"))
                    .unwrap_or_default();
                (format!("{}{}", explain.provider, model), Severity::Healthy)
            }
            ExplainStatus::DisabledKeyDetected { env_var } => {
                (format!("disabled ({env_var} detected)"), Severity::Stale)
            }
            ExplainStatus::Disabled => ("disabled".to_string(), Severity::Healthy),
        };
        rows.push(HealthRow {
            label: "explain".to_string(),
            value,
            severity,
        });

        if let Some(totals) = &snapshot.explain_totals {
            let calls = totals.calls + totals.failures + totals.budget_blocked;
            if calls > 0 {
                let openrouter_live = totals
                    .per_provider
                    .get("openrouter")
                    .and_then(|provider| provider.usd_cost)
                    .is_some();
                let cost = if totals.any_unpriced {
                    format!("${:.4} (+ unpriced)", totals.usd_cost)
                } else {
                    format!("${:.4}", totals.usd_cost)
                };
                let tokens = if totals.any_estimated {
                    format!(
                        "{} in / {} out (est.)",
                        totals.input_tokens, totals.output_tokens
                    )
                } else {
                    format!("{} in / {} out", totals.input_tokens, totals.output_tokens)
                };
                rows.push(HealthRow {
                    label: "explain usage".to_string(),
                    value: format!(
                        "{} calls · {} · {} ({})",
                        calls,
                        tokens,
                        cost,
                        crate::pipeline::explain::pricing::pricing_basis_label(openrouter_live)
                    ),
                    severity: Severity::Healthy,
                });
                if totals.failures > 0 || totals.budget_blocked > 0 {
                    rows.push(HealthRow {
                        label: "explain skipped".to_string(),
                        value: format!(
                            "{} failed · {} budget-blocked",
                            totals.failures, totals.budget_blocked
                        ),
                        severity: if totals.failures > 0 {
                            Severity::Stale
                        } else {
                            Severity::Healthy
                        },
                    });
                }
            }
        }
    }

    HealthVm { rows }
}

/// Recent-activity entry reshaped for rendering.
#[derive(Clone, Debug)]
pub struct ActivityVmEntry {
    /// RFC-3339 timestamp; empty when unknown.
    pub timestamp: String,
    /// Short event kind tag.
    pub kind: String,
    /// One-line payload.
    pub payload: String,
}

/// Recent-activity view model.
#[derive(Clone, Debug, Default)]
pub struct ActivityVm {
    /// Entries newest-first.
    pub entries: Vec<ActivityVmEntry>,
}

/// Build a recent-activity view model. Uses snapshot entries when the caller
/// already opted into `recent`; otherwise returns empty.
pub fn build_activity_vm(snapshot: &StatusSnapshot) -> ActivityVm {
    let Some(entries) = &snapshot.recent_activity else {
        return ActivityVm::default();
    };
    ActivityVm {
        entries: entries
            .iter()
            .map(|e| ActivityVmEntry {
                timestamp: e.timestamp.clone(),
                kind: e.kind.clone(),
                payload: e.payload.to_string(),
            })
            .collect(),
    }
}

/// One recommended next-action derived from health signals.
#[derive(Clone, Debug)]
pub struct NextAction {
    /// Short label.
    pub label: String,
    /// Severity-driven ordering hint.
    pub severity: Severity,
}

/// Derive next-actions from a snapshot + integration signal.
pub fn build_next_actions(
    snapshot: &StatusSnapshot,
    integration: &AgentIntegration,
) -> Vec<NextAction> {
    let mut out: Vec<NextAction> = Vec::new();

    if !snapshot.initialized {
        out.push(NextAction {
            label: "Run `synrepo init` to set up this repo".to_string(),
            severity: Severity::Blocked,
        });
        return out;
    }

    if snapshot.graph_stats.is_none() {
        out.push(NextAction {
            label: "Graph not materialized — run `synrepo reconcile`".to_string(),
            severity: Severity::Blocked,
        });
    }

    if let Some(d) = &snapshot.diagnostics {
        match &d.reconcile_health {
            ReconcileHealth::Stale(_) | ReconcileHealth::Unknown => {
                out.push(NextAction {
                    label: "Reconcile is stale — refresh with `synrepo reconcile`".to_string(),
                    severity: Severity::Stale,
                });
            }
            ReconcileHealth::Corrupt(_) => {
                out.push(NextAction {
                    label: "Reconcile state corrupt — run `synrepo watch stop`".to_string(),
                    severity: Severity::Blocked,
                });
            }
            ReconcileHealth::Current => {}
        }
        if !d.store_guidance.is_empty() {
            out.push(NextAction {
                label: format!("Store compat action: {}", d.store_guidance[0]),
                severity: Severity::Blocked,
            });
        }
    }

    if !snapshot.export_freshness.starts_with("current") {
        out.push(NextAction {
            label: "Export is stale — run `synrepo export`".to_string(),
            severity: Severity::Stale,
        });
    }

    match integration {
        AgentIntegration::Absent => {
            out.push(NextAction {
                label: "Agent integration absent — open integration flow".to_string(),
                severity: Severity::Stale,
            });
        }
        AgentIntegration::Partial { target } => {
            out.push(NextAction {
                label: format!(
                    "Complete MCP registration for {}",
                    integration_target_label(*target)
                ),
                severity: Severity::Stale,
            });
        }
        AgentIntegration::Complete { .. } => {}
    }

    if out.is_empty() {
        out.push(NextAction {
            label: "All systems healthy. Connect an agent via MCP.".to_string(),
            severity: Severity::Healthy,
        });
    }
    out
}

fn integration_target_label(target: AgentTargetKind) -> &'static str {
    target.as_str()
}

/// View model for the resolved repo path, used by the header.
pub fn display_repo_path(p: &std::path::Path) -> String {
    shorten_home(p)
}

fn shorten_home(p: &std::path::Path) -> String {
    let full = p.display().to_string();
    let home = home_dir();
    if let Some(home) = home {
        let home_str = home.display().to_string();
        if full.starts_with(&home_str) {
            return full.replacen(&home_str, "~", 1);
        }
    }
    full
}
