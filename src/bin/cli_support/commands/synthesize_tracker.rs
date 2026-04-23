use std::collections::HashMap;

use crossbeam_channel::Receiver;
use synrepo::pipeline::synthesis::{
    pricing,
    telemetry::{SynthesisEvent, SynthesisTarget, UsageSource},
};

#[derive(Clone, Debug)]
pub(super) struct RenderedStatus {
    outcome: RenderedOutcome,
    detail: String,
}

impl RenderedStatus {
    pub(super) fn generated(detail: String) -> Self {
        Self {
            outcome: RenderedOutcome::Generated,
            detail,
        }
    }

    pub(super) fn skipped(detail: String) -> Self {
        Self {
            outcome: RenderedOutcome::Skipped,
            detail,
        }
    }

    pub(super) fn failed(detail: String) -> Self {
        Self {
            outcome: RenderedOutcome::Failed,
            detail,
        }
    }

    pub(super) fn headline(&self, success_label: &'static str) -> &'static str {
        match self.outcome {
            RenderedOutcome::Generated => success_label,
            RenderedOutcome::Skipped => "skipped",
            RenderedOutcome::Failed => "failed",
        }
    }

    pub(super) fn detail(&self) -> &str {
        &self.detail
    }
}

#[derive(Clone, Copy, Debug)]
enum RenderedOutcome {
    Generated,
    Skipped,
    Failed,
}

pub(super) struct TelemetryTracker {
    rx: Receiver<SynthesisEvent>,
    terminal_by_node: HashMap<String, TerminalEvent>,
    max_targets: Option<usize>,
    active_calls: u64,
    calls: u64,
    failures: u64,
    budget_blocked: u64,
    input_tokens: u64,
    output_tokens: u64,
    usd_cost: f64,
    unpriced_calls: u64,
    any_estimated: bool,
}

impl TelemetryTracker {
    pub(super) fn new(rx: Receiver<SynthesisEvent>) -> Self {
        Self {
            rx,
            terminal_by_node: HashMap::new(),
            max_targets: None,
            active_calls: 0,
            calls: 0,
            failures: 0,
            budget_blocked: 0,
            input_tokens: 0,
            output_tokens: 0,
            usd_cost: 0.0,
            unpriced_calls: 0,
            any_estimated: false,
        }
    }

    pub(super) fn empty() -> Self {
        let (_tx, rx) = crossbeam_channel::bounded(1);
        Self::new(rx)
    }

    pub(super) fn calls(&self) -> u64 {
        self.calls
    }

    pub(super) fn failures(&self) -> u64 {
        self.failures
    }

    pub(super) fn budget_blocked(&self) -> u64 {
        self.budget_blocked
    }

    pub(super) fn input_tokens(&self) -> u64 {
        self.input_tokens
    }

    pub(super) fn output_tokens(&self) -> u64 {
        self.output_tokens
    }

    pub(super) fn unpriced_calls(&self) -> u64 {
        self.unpriced_calls
    }

    pub(super) fn usd_cost(&self) -> f64 {
        self.usd_cost
    }

    pub(super) fn any_estimated(&self) -> bool {
        self.any_estimated
    }

    pub(super) fn drain(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                SynthesisEvent::CallStarted { .. } => {
                    self.active_calls += 1;
                }
                SynthesisEvent::CallCompleted {
                    provider,
                    model,
                    target,
                    duration_ms,
                    usage,
                    billed_usd_cost,
                    ..
                } => {
                    self.calls += 1;
                    self.input_tokens += usage.input_tokens as u64;
                    self.output_tokens += usage.output_tokens as u64;
                    if usage.source == UsageSource::Estimated {
                        self.any_estimated = true;
                    }
                    match billed_usd_cost.or_else(|| {
                        pricing::cost_for_call(
                            provider,
                            &model,
                            usage.input_tokens,
                            usage.output_tokens,
                        )
                    }) {
                        Some(cost) => self.usd_cost += cost,
                        None => self.unpriced_calls += 1,
                    }
                    if let Some(node_id) = commentary_target_key(&target) {
                        self.terminal_by_node.insert(
                            node_id,
                            TerminalEvent::Completed {
                                duration_ms,
                                input_tokens: usage.input_tokens,
                                output_tokens: usage.output_tokens,
                                estimated: usage.source == UsageSource::Estimated,
                            },
                        );
                    }
                    self.finish_call();
                }
                SynthesisEvent::BudgetBlocked {
                    target,
                    estimated_tokens,
                    budget,
                    ..
                } => {
                    self.budget_blocked += 1;
                    if let Some(node_id) = commentary_target_key(&target) {
                        self.terminal_by_node.insert(
                            node_id,
                            TerminalEvent::BudgetBlocked {
                                estimated_tokens,
                                budget,
                            },
                        );
                    }
                    self.finish_call();
                }
                SynthesisEvent::CallFailed {
                    target,
                    duration_ms,
                    error,
                    ..
                } => {
                    self.failures += 1;
                    if let Some(node_id) = commentary_target_key(&target) {
                        self.terminal_by_node
                            .insert(node_id, TerminalEvent::Failed { duration_ms, error });
                    }
                    self.finish_call();
                }
            }
        }
    }

    pub(super) fn note_plan(&mut self, max_targets: usize) {
        self.max_targets = Some(max_targets);
    }

    pub(super) fn render_counter(&self, current: usize) -> String {
        match self.max_targets {
            Some(max_targets) => format!("[{current} / <= {max_targets}]"),
            None => format!("[{current}]"),
        }
    }

    pub(super) fn total_calls(&self) -> u64 {
        self.calls + self.failures + self.budget_blocked
    }

    pub(super) fn summary_label(&self) -> String {
        let mut summary = format!(
            "{} call(s), {} ok, {} failed, {} budget blocked, {} in / {} out",
            self.total_calls(),
            self.calls,
            self.failures,
            self.budget_blocked,
            self.input_tokens,
            self.output_tokens
        );
        if self.any_estimated {
            summary.push_str(", estimated tokens");
        }
        if self.unpriced_calls > 0 {
            summary.push_str(&format!(", {} unpriced", self.unpriced_calls));
        } else if self.total_calls() > 0 {
            summary.push_str(&format!(", ${:.4}", self.usd_cost));
        }
        summary
    }

    pub(super) fn usage_label(&self) -> String {
        if self.max_targets == Some(0) && self.active_calls == 0 && self.total_calls() == 0 {
            return "no provider calls needed for this scope".to_string();
        }
        if self.active_calls == 0 {
            if self.total_calls() == 0 {
                return "waiting for first provider response".to_string();
            }
            return self.summary_label();
        }
        if self.total_calls() == 0 {
            return format!(
                "{} provider call(s) in flight, waiting for first response",
                self.active_calls
            );
        }
        format!("{}, {} in flight", self.summary_label(), self.active_calls)
    }

    pub(super) fn take_status(
        &mut self,
        node_id: synrepo::NodeId,
        generated: bool,
    ) -> RenderedStatus {
        match self.terminal_by_node.remove(&node_id.to_string()) {
            Some(TerminalEvent::Completed {
                duration_ms,
                input_tokens,
                output_tokens,
                estimated,
            }) if generated => {
                let est = if estimated { " est." } else { "" };
                RenderedStatus::generated(format!(
                    "ok ({duration_ms}ms, {input_tokens} in / {output_tokens} out{est})"
                ))
            }
            Some(TerminalEvent::Completed { .. }) => {
                RenderedStatus::skipped("provider returned no commentary".to_string())
            }
            Some(TerminalEvent::BudgetBlocked {
                estimated_tokens,
                budget,
            }) => RenderedStatus::skipped(format!(
                "budget blocked ({estimated_tokens} est. > {budget})"
            )),
            Some(TerminalEvent::Failed { duration_ms, error }) => {
                RenderedStatus::failed(format!("failed ({duration_ms}ms): {error}"))
            }
            None if generated => RenderedStatus::generated("ok".to_string()),
            None => RenderedStatus::skipped("provider returned no commentary".to_string()),
        }
    }

    fn finish_call(&mut self) {
        self.active_calls = self.active_calls.saturating_sub(1);
    }
}

#[derive(Clone, Debug)]
enum TerminalEvent {
    Completed {
        duration_ms: u64,
        input_tokens: u32,
        output_tokens: u32,
        estimated: bool,
    },
    BudgetBlocked {
        estimated_tokens: u32,
        budget: u32,
    },
    Failed {
        duration_ms: u64,
        error: String,
    },
}

fn commentary_target_key(target: &SynthesisTarget) -> Option<String> {
    match target {
        SynthesisTarget::Commentary { node } => Some(node.to_string()),
        SynthesisTarget::CrossLink { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synrepo::core::ids::{NodeId, SymbolNodeId};
    use synrepo::pipeline::synthesis::telemetry::SynthesisTarget;

    fn sample_target() -> SynthesisTarget {
        SynthesisTarget::Commentary {
            node: NodeId::Symbol(SymbolNodeId(1)),
        }
    }

    #[test]
    fn usage_label_reports_in_flight_before_first_response() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut tracker = TelemetryTracker::new(rx);
        tx.send(SynthesisEvent::CallStarted {
            call_id: 1,
            provider: "minimax",
            model: "MiniMax-M2".to_string(),
            target: sample_target(),
            started_at_ms: 0,
        })
        .expect("send started event");
        tracker.drain();
        assert_eq!(
            tracker.usage_label(),
            "1 provider call(s) in flight, waiting for first response"
        );
    }
}
