//! Mapping from `SynthesisEvent` (process-global synthesis fan-out) to the
//! log-pane `LogEntry`. Lives alongside `watch_events.rs` so the tag/severity
//! contract can be pinned by unit tests without constructing an `AppState`.

use crate::pipeline::synthesis::telemetry::{SynthesisEvent, SynthesisTarget, UsageSource};
use crate::pipeline::writer::now_rfc3339;
use crate::tui::probe::Severity;
use crate::tui::widgets::LogEntry;

/// Pure mapping from a `SynthesisEvent` to a `LogEntry`. `CallStarted` is
/// dropped (too noisy in the live feed); every terminal event produces one
/// line.
pub fn synthesis_event_to_log_entry(event: SynthesisEvent) -> Option<LogEntry> {
    let tag = "synthesis".to_string();
    let ts = now_rfc3339();
    let entry = match event {
        SynthesisEvent::CallStarted { .. } => return None,
        SynthesisEvent::CallCompleted {
            call_id,
            provider,
            model: _,
            target,
            duration_ms,
            usage,
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
        SynthesisEvent::BudgetBlocked {
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
        SynthesisEvent::CallFailed {
            call_id,
            provider,
            model: _,
            target,
            duration_ms,
            error,
        } => LogEntry {
            timestamp: ts,
            tag,
            message: format!(
                "#{call_id} {provider} {target} failed ({duration_ms}ms): {error}",
                target = target_label(&target),
            ),
            severity: Severity::Blocked,
        },
    };
    Some(entry)
}

fn target_label(target: &SynthesisTarget) -> String {
    match target {
        SynthesisTarget::Commentary { node } => format!("commentary({node})"),
        SynthesisTarget::CrossLink { from, to, kind: _ } => {
            format!("cross_link({from}→{to})")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{NodeId, SymbolNodeId};
    use crate::pipeline::synthesis::telemetry::TokenUsage;

    fn sample_node() -> NodeId {
        NodeId::Symbol(SymbolNodeId(1))
    }

    #[test]
    fn call_started_is_dropped() {
        let out = synthesis_event_to_log_entry(SynthesisEvent::CallStarted {
            call_id: 1,
            provider: "anthropic",
            model: "claude-sonnet-4-6".to_string(),
            target: SynthesisTarget::Commentary {
                node: sample_node(),
            },
            started_at_ms: 0,
        });
        assert!(out.is_none(), "CallStarted should not clutter the feed");
    }

    #[test]
    fn call_completed_reported_is_healthy() {
        let entry = synthesis_event_to_log_entry(SynthesisEvent::CallCompleted {
            call_id: 7,
            provider: "anthropic",
            model: "claude-sonnet-4-6".to_string(),
            target: SynthesisTarget::Commentary {
                node: sample_node(),
            },
            duration_ms: 123,
            usage: TokenUsage::reported(100, 200),
            output_bytes: 512,
        })
        .expect("CallCompleted must produce an entry");
        assert_eq!(entry.tag, "synthesis");
        assert!(entry.message.contains("ok"));
        assert!(entry.message.contains("100 in / 200 out"));
        assert!(!entry.message.contains("est."));
        assert!(matches!(entry.severity, Severity::Healthy));
    }

    #[test]
    fn call_completed_estimated_is_stale() {
        let entry = synthesis_event_to_log_entry(SynthesisEvent::CallCompleted {
            call_id: 8,
            provider: "local",
            model: "llama3".to_string(),
            target: SynthesisTarget::Commentary {
                node: sample_node(),
            },
            duration_ms: 400,
            usage: TokenUsage::estimated(50, 80),
            output_bytes: 256,
        })
        .expect("entry");
        assert!(entry.message.contains("est."));
        assert!(matches!(entry.severity, Severity::Stale));
    }

    #[test]
    fn budget_blocked_is_stale() {
        let entry = synthesis_event_to_log_entry(SynthesisEvent::BudgetBlocked {
            call_id: 9,
            provider: "openai",
            model: "gpt-4o-mini".to_string(),
            target: SynthesisTarget::Commentary {
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
        let entry = synthesis_event_to_log_entry(SynthesisEvent::CallFailed {
            call_id: 10,
            provider: "anthropic",
            model: "claude-sonnet-4-6".to_string(),
            target: SynthesisTarget::Commentary {
                node: sample_node(),
            },
            duration_ms: 42,
            error: "transport error: connect refused".to_string(),
        })
        .expect("entry");
        assert!(entry.message.contains("failed"));
        assert!(entry.message.contains("transport error"));
        assert!(matches!(entry.severity, Severity::Blocked));
    }
}
