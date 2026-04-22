use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::super::pricing;
use super::super::telemetry::{Outcome, SynthesisEvent, SynthesisTarget, UsageSource};
use super::types::SynthesisCallRecord;

pub(super) fn record_for_event(event: &SynthesisEvent) -> Option<SynthesisCallRecord> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default();

    match event {
        SynthesisEvent::CallStarted { .. } => None,
        SynthesisEvent::CallCompleted {
            call_id,
            provider,
            model,
            target,
            duration_ms,
            usage,
            billed_usd_cost,
            output_bytes: _,
        } => Some(SynthesisCallRecord {
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
        SynthesisEvent::BudgetBlocked {
            call_id,
            provider,
            model,
            target,
            estimated_tokens,
            budget: _,
        } => Some(SynthesisCallRecord {
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
        SynthesisEvent::CallFailed {
            call_id,
            provider,
            model,
            target,
            duration_ms,
            error,
        } => Some(SynthesisCallRecord {
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

fn target_kind_label(target: &SynthesisTarget) -> &'static str {
    match target {
        SynthesisTarget::Commentary { .. } => "commentary",
        SynthesisTarget::CrossLink { .. } => "cross_link",
    }
}
