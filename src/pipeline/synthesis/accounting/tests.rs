use super::super::telemetry::{SynthesisEvent, SynthesisTarget, TokenUsage};
use super::*;
use crate::core::ids::{FileNodeId, NodeId};
use tempfile::tempdir;

fn file_target(n: u128) -> SynthesisTarget {
    SynthesisTarget::Commentary {
        node: NodeId::File(FileNodeId(n)),
    }
}

fn completed(call_id: u64, usage: TokenUsage) -> SynthesisEvent {
    SynthesisEvent::CallCompleted {
        call_id,
        provider: "openai",
        model: "gpt-4o-mini".to_string(),
        target: file_target(call_id as u128),
        duration_ms: 100,
        usage,
        billed_usd_cost: None,
        output_bytes: 42,
    }
}

#[test]
fn completed_event_appends_and_updates_totals() {
    let dir = tempdir().unwrap();
    let synrepo = dir.path();

    record_event(
        synrepo,
        &completed(1, TokenUsage::reported(1_000_000, 1_000_000)),
    )
    .unwrap();

    let log = std::fs::read_to_string(log_path(synrepo)).unwrap();
    assert_eq!(log.lines().count(), 1);
    let rec: SynthesisCallRecord = serde_json::from_str(log.lines().next().unwrap()).unwrap();
    assert_eq!(rec.provider, "openai");
    assert_eq!(rec.outcome, "success");
    assert!(rec.usd_cost.is_some());

    let totals = load_totals(synrepo).unwrap().unwrap();
    assert_eq!(totals.calls, 1);
    assert_eq!(totals.input_tokens, 1_000_000);
    assert!(!totals.any_estimated);
    assert!(!totals.any_unpriced);
    let per = totals.per_provider.get("openai").unwrap();
    assert_eq!(per.calls, 1);
}

#[test]
fn estimated_usage_flips_any_estimated_bit() {
    let dir = tempdir().unwrap();
    record_event(dir.path(), &completed(1, TokenUsage::estimated(100, 100))).unwrap();
    let totals = load_totals(dir.path()).unwrap().unwrap();
    assert!(totals.any_estimated);
}

#[test]
fn unknown_model_flips_any_unpriced_bit_and_records_null_cost() {
    let dir = tempdir().unwrap();
    let event = SynthesisEvent::CallCompleted {
        call_id: 1,
        provider: "openai",
        model: "unknown-future-model".to_string(),
        target: file_target(1),
        duration_ms: 10,
        usage: TokenUsage::reported(100, 100),
        billed_usd_cost: None,
        output_bytes: 5,
    };
    record_event(dir.path(), &event).unwrap();
    let totals = load_totals(dir.path()).unwrap().unwrap();
    assert!(totals.any_unpriced);
    let log = std::fs::read_to_string(log_path(dir.path())).unwrap();
    let rec: SynthesisCallRecord = serde_json::from_str(log.lines().next().unwrap()).unwrap();
    assert!(rec.usd_cost.is_none());
}

#[test]
fn failed_event_records_error_and_bumps_failures() {
    let dir = tempdir().unwrap();
    let event = SynthesisEvent::CallFailed {
        call_id: 1,
        provider: "openai",
        model: "gpt-4o-mini".to_string(),
        target: file_target(1),
        duration_ms: 250,
        error: "HTTP 500: upstream down".to_string(),
    };
    record_event(dir.path(), &event).unwrap();
    let totals = load_totals(dir.path()).unwrap().unwrap();
    assert_eq!(totals.failures, 1);
    assert_eq!(totals.calls, 0);
    let log = std::fs::read_to_string(log_path(dir.path())).unwrap();
    let rec: SynthesisCallRecord = serde_json::from_str(log.lines().next().unwrap()).unwrap();
    assert!(rec.error_tail.contains("HTTP 500"));
}

#[test]
fn reset_rotates_log_and_zeros_totals() {
    let dir = tempdir().unwrap();
    record_event(dir.path(), &completed(1, TokenUsage::reported(10, 10))).unwrap();
    reset(dir.path()).unwrap();

    assert!(!log_path(dir.path()).exists(), "log should be rotated");
    let state = dir.path().join("state");
    let baks: Vec<_> = std::fs::read_dir(state)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".bak"))
        .collect();
    assert_eq!(baks.len(), 1);

    let totals = load_totals(dir.path()).unwrap().unwrap();
    assert_eq!(totals.calls, 0);
    assert_eq!(totals.input_tokens, 0);
    assert!(totals.since.is_some());
}

#[test]
fn call_started_events_are_not_logged() {
    let dir = tempdir().unwrap();
    let event = SynthesisEvent::CallStarted {
        call_id: 1,
        provider: "openai",
        model: "gpt-4o-mini".to_string(),
        target: file_target(1),
        started_at_ms: 0,
    };
    record_event(dir.path(), &event).unwrap();
    assert!(!log_path(dir.path()).exists());
    assert!(!totals_path(dir.path()).exists());
}
