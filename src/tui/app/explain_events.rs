//! Mapping from `ExplainEvent` (process-global explain fan-out) to the
//! log-pane `LogEntry`. Lives alongside `watch_events.rs` so the tag/severity
//! contract can be pinned by unit tests without constructing an `AppState`.

use crate::pipeline::explain::telemetry::{ExplainEvent, ExplainTarget, UsageSource};
use crate::pipeline::writer::now_rfc3339;
use crate::tui::probe::Severity;
use crate::tui::widgets::LogEntry;

/// Pure mapping from a `ExplainEvent` to a `LogEntry`. `CallStarted` is
/// dropped (too noisy in the live feed); every terminal event produces one
/// line.
pub fn explain_event_to_log_entry(event: ExplainEvent) -> Option<LogEntry> {
    let tag = "explain".to_string();
    let ts = now_rfc3339();
    let entry = match event {
        ExplainEvent::CallStarted { .. } => return None,
        ExplainEvent::CallCompleted {
            call_id,
            provider,
            model: _,
            target,
            duration_ms,
            usage,
            billed_usd_cost: _,
            output_bytes: _,
        } => {
            let est = match usage.source {
                UsageSource::Reported => "",
                UsageSource::Estimated => " est.",
            };
            LogEntry {
                timestamp: ts,
                tag,
                message: format!(
                    "#{call_id} {provider} {target} ok ({duration_ms}ms, {in_tok} in / {out_tok} out{est})",
                    target = target_label(&target),
                    in_tok = usage.input_tokens,
                    out_tok = usage.output_tokens,
                ),
                severity: match usage.source {
                    UsageSource::Reported => Severity::Healthy,
                    UsageSource::Estimated => Severity::Stale,
                },
            }
        }
        ExplainEvent::BudgetBlocked {
            call_id,
            provider,
            model: _,
            target,
            estimated_tokens,
            budget,
        } => LogEntry {
            timestamp: ts,
            tag,
            message: format!(
                "#{call_id} {provider} {target} skipped: {estimated_tokens} est. tokens > {budget} budget",
                target = target_label(&target),
            ),
            severity: Severity::Stale,
        },
        ExplainEvent::CallFailed {
            call_id,
            provider,
            model: _,
            target,
            duration_ms,
            error,
            http_status,
            retry_after_ms,
        } => LogEntry {
            timestamp: ts,
            tag,
            message: failure_message(
                call_id,
                provider,
                &target,
                duration_ms,
                &error,
                http_status,
                retry_after_ms,
            ),
            severity: Severity::Blocked,
        },
    };
    Some(entry)
}

fn failure_message(
    call_id: u64,
    provider: &str,
    target: &ExplainTarget,
    duration_ms: u64,
    error: &str,
    http_status: Option<u16>,
    retry_after_ms: Option<u64>,
) -> String {
    let target = target_label(target);
    if http_status == Some(429) {
        let retry = retry_after_ms
            .map(|ms| format!(", retry after {}ms", ms))
            .unwrap_or_default();
        return format!(
            "#{call_id} {provider} {target} rate limited ({duration_ms}ms{retry}): {error}"
        );
    }
    format!("#{call_id} {provider} {target} failed ({duration_ms}ms): {error}")
}

fn target_label(target: &ExplainTarget) -> String {
    match target {
        ExplainTarget::Commentary { node } => format!("commentary({node})"),
        ExplainTarget::CrossLink { from, to, kind: _ } => {
            format!("cross_link({from}→{to})")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{NodeId, SymbolNodeId};
    use crate::pipeline::explain::telemetry::TokenUsage;

    fn sample_node() -> NodeId {
        NodeId::Symbol(SymbolNodeId(1))
    }

    #[test]
    fn call_started_is_dropped() {
        let out = explain_event_to_log_entry(ExplainEvent::CallStarted {
            call_id: 1,
            provider: "anthropic",
            model: "claude-sonnet-4-6".to_string(),
            target: ExplainTarget::Commentary {
                node: sample_node(),
            },
            started_at_ms: 0,
        });
        assert!(out.is_none(), "CallStarted should not clutter the feed");
    }

    #[test]
    fn call_completed_reported_is_healthy() {
        let entry = explain_event_to_log_entry(ExplainEvent::CallCompleted {
            call_id: 7,
            provider: "anthropic",
            model: "claude-sonnet-4-6".to_string(),
            target: ExplainTarget::Commentary {
                node: sample_node(),
            },
            duration_ms: 123,
            usage: TokenUsage::reported(100, 200),
            billed_usd_cost: None,
            output_bytes: 512,
        })
        .expect("CallCompleted must produce an entry");
        assert_eq!(entry.tag, "explain");
        assert!(entry.message.contains("ok"));
        assert!(entry.message.contains("100 in / 200 out"));
        assert!(!entry.message.contains("est."));
        assert!(matches!(entry.severity, Severity::Healthy));
    }

    #[test]
    fn call_completed_estimated_is_stale() {
        let entry = explain_event_to_log_entry(ExplainEvent::CallCompleted {
            call_id: 8,
            provider: "local",
            model: "llama3".to_string(),
            target: ExplainTarget::Commentary {
                node: sample_node(),
            },
            duration_ms: 400,
            usage: TokenUsage::estimated(50, 80),
            billed_usd_cost: None,
            output_bytes: 256,
        })
        .expect("entry");
        assert!(entry.message.contains("est."));
        assert!(matches!(entry.severity, Severity::Stale));
    }

    #[test]
    fn budget_blocked_is_stale() {
        let entry = explain_event_to_log_entry(ExplainEvent::BudgetBlocked {
            call_id: 9,
            provider: "openai",
            model: "gpt-4o-mini".to_string(),
            target: ExplainTarget::Commentary {
                node: sample_node(),
            },
            estimated_tokens: 6000,
            budget: 5000,
        })
        .expect("entry");
        assert!(entry.message.contains("skipped"));
        assert!(entry.message.contains("6000"));
        assert!(matches!(entry.severity, Severity::Stale));
    }

    #[test]
    fn call_failed_is_blocked() {
        let entry = explain_event_to_log_entry(ExplainEvent::CallFailed {
            call_id: 10,
            provider: "anthropic",
            model: "claude-sonnet-4-6".to_string(),
            target: ExplainTarget::Commentary {
                node: sample_node(),
            },
            duration_ms: 42,
            error: "transport error: connect refused".to_string(),
            http_status: None,
            retry_after_ms: None,
        })
        .expect("entry");
        assert!(entry.message.contains("failed"));
        assert!(entry.message.contains("transport error"));
        assert!(matches!(entry.severity, Severity::Blocked));
    }

    #[test]
    fn call_failed_429_is_labeled_rate_limited() {
        let entry = explain_event_to_log_entry(ExplainEvent::CallFailed {
            call_id: 11,
            provider: "zai",
            model: "glm-4.6".to_string(),
            target: ExplainTarget::Commentary {
                node: sample_node(),
            },
            duration_ms: 42,
            error: "non-success status: 429 Too Many Requests".to_string(),
            http_status: Some(429),
            retry_after_ms: Some(2000),
        })
        .expect("entry");
        assert!(entry.message.contains("rate limited"));
        assert!(entry.message.contains("retry after 2000ms"));
        assert!(matches!(entry.severity, Severity::Blocked));
    }
}
