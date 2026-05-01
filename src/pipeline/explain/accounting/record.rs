use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::super::pricing;
use super::super::telemetry::{ExplainEvent, ExplainTarget, Outcome, UsageSource};
use super::types::ExplainCallRecord;

pub(super) fn record_for_event(event: &ExplainEvent) -> Option<ExplainCallRecord> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default();

    match event {
        ExplainEvent::CallStarted { .. } => None,
        ExplainEvent::CallCompleted {
            call_id,
            provider,
            model,
            target,
            duration_ms,
            usage,
            billed_usd_cost,
            output_bytes: _,
        } => Some(ExplainCallRecord {
            timestamp: now,
            call_id: *call_id,
            provider: (*provider).to_string(),
            model: model.clone(),
            target_kind: target_kind_label(target).to_string(),
            target_label: target.display_label(),
            outcome: Outcome::Success.as_str().to_string(),
            duration_ms: *duration_ms,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            usage_source: usage.source.as_str().to_string(),
            usd_cost: billed_usd_cost.or_else(|| {
                pricing::cost_for_call(provider, model, usage.input_tokens, usage.output_tokens)
            }),
            error_tail: String::new(),
        }),
        ExplainEvent::BudgetBlocked {
            call_id,
            provider,
            model,
            target,
            estimated_tokens,
            budget: _,
        } => Some(ExplainCallRecord {
            timestamp: now,
            call_id: *call_id,
            provider: (*provider).to_string(),
            model: model.clone(),
            target_kind: target_kind_label(target).to_string(),
            target_label: target.display_label(),
            outcome: Outcome::BudgetBlocked.as_str().to_string(),
            duration_ms: 0,
            input_tokens: *estimated_tokens,
            output_tokens: 0,
            usage_source: UsageSource::Estimated.as_str().to_string(),
            usd_cost: None,
            error_tail: String::new(),
        }),
        ExplainEvent::CallFailed {
            call_id,
            provider,
            model,
            target,
            duration_ms,
            error,
            ..
        } => Some(ExplainCallRecord {
            timestamp: now,
            call_id: *call_id,
            provider: (*provider).to_string(),
            model: model.clone(),
            target_kind: target_kind_label(target).to_string(),
            target_label: target.display_label(),
            outcome: Outcome::Failed.as_str().to_string(),
            duration_ms: *duration_ms,
            input_tokens: 0,
            output_tokens: 0,
            usage_source: String::new(),
            usd_cost: None,
            error_tail: error.clone(),
        }),
    }
}

fn target_kind_label(target: &ExplainTarget) -> &'static str {
    match target {
        ExplainTarget::Commentary { .. } => "commentary",
        ExplainTarget::CrossLink { .. } => "cross_link",
    }
}
