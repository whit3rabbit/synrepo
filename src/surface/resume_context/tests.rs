use serde_json::json;
use tempfile::tempdir;

use crate::{
    bootstrap::bootstrap,
    config::Config,
    overlay::{AgentNote, AgentNoteConfidence, AgentNoteTarget, AgentNoteTargetKind, OverlayStore},
    pipeline::recent_activity::ActivityEntry,
    store::overlay::SqliteOverlayStore,
    surface::{
        handoffs::{HandoffItem, HandoffPriority, HandoffSource},
        resume_context::{
            AgentNoteSummary, ChangedFilesSection, NextActionsSection, RecentActivitySection,
            ResumeContextPacket, ResumeContextRequest, ResumeContextSections, ResumeContextState,
            SavedNotesSection, ValidationSection,
        },
    },
};

use super::{budget::apply_budget, build_resume_context, notes::read_saved_notes, to_json};

#[test]
fn empty_state_packet_is_valid_and_records_aggregate_metrics() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), None, false).unwrap();
    let config = Config::load(repo.path()).unwrap();

    let packet = build_resume_context(
        repo.path(),
        &config,
        ResumeContextRequest {
            include_notes: false,
            ..ResumeContextRequest::default()
        },
    )
    .unwrap();

    assert_eq!(packet.schema_version, 1);
    assert_eq!(packet.packet_type, "repo_resume_context");
    assert!(packet.sections.changed_files.files.is_empty());
    assert_eq!(packet.sections.saved_notes.overlay_state, "disabled");
    assert!(packet
        .sections
        .validation
        .recommended_commands
        .contains(&"synrepo status --recent".to_string()));

    let metrics = crate::pipeline::context_metrics::load(&Config::synrepo_dir(repo.path()))
        .expect("metrics should be persisted");
    assert_eq!(metrics.resume_context_responses_total, 1);
    assert!(metrics.resume_context_tokens_total > 0);
}

#[test]
fn saved_notes_are_summary_only() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), None, false).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let claim = format!(
        "{}TAIL_DO_NOT_INCLUDE",
        "A".repeat(super::notes::NOTE_PREVIEW_CHARS + 10)
    );
    overlay
        .insert_note(AgentNote::new(
            AgentNoteTarget {
                kind: AgentNoteTargetKind::Path,
                id: "src/lib.rs".to_string(),
            },
            claim,
            "codex".to_string(),
            AgentNoteConfidence::Medium,
        ))
        .unwrap();

    let config = Config::load(repo.path()).unwrap();
    let packet = build_resume_context(repo.path(), &config, ResumeContextRequest::default())
        .expect("packet should build");
    let note = packet
        .sections
        .saved_notes
        .notes
        .first()
        .expect("note summary should be present");

    assert_eq!(note.target_kind, "path");
    assert_eq!(note.target, "src/lib.rs");
    assert!(note.claim_preview.ends_with("..."));
    assert!(!to_json(&packet).contains("TAIL_DO_NOT_INCLUDE"));
}

#[test]
fn overlay_unavailable_is_reported_without_failing_packet() {
    let dir = tempdir().unwrap();

    let notes = read_saved_notes(dir.path(), true, 10);

    assert_eq!(notes.overlay_state, "unavailable");
    assert!(notes.overlay_error.is_some());
    assert!(notes.notes.is_empty());
}

#[test]
fn budget_omits_lower_priority_sections_before_critical_state() {
    let mut packet = synthetic_packet_with_large_sections();
    packet.context_state.token_cap = 1;

    apply_budget(&mut packet);

    assert!(packet.context_state.truncation_applied);
    assert!(packet.context_state.metrics_hint.is_none());
    assert!(packet.sections.recent_activity.activity.is_empty());
    assert!(packet.sections.saved_notes.notes.is_empty());
    assert!(packet.sections.next_actions.items.is_empty());
    assert_eq!(packet.sections.changed_files.files.len(), 5);
    assert!(!packet.sections.validation.recommended_commands.is_empty());
    let omitted = packet
        .omitted
        .iter()
        .map(|item| item.section.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        omitted,
        vec![
            "metrics_hint",
            "recent_activity",
            "saved_notes",
            "next_actions",
            "changed_files"
        ]
    );
}

#[test]
fn packet_shape_has_no_generic_session_memory_fields() {
    let packet = synthetic_packet_with_large_sections();
    let serialized = to_json(&packet);

    for forbidden in [
        "prompt",
        "chat_history",
        "raw_tool_output",
        "tool_output",
        "session_history",
        "caller_identity",
        "response_body",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "packet must not expose generic session-memory field {forbidden}"
        );
    }
}

fn synthetic_packet_with_large_sections() -> ResumeContextPacket {
    ResumeContextPacket {
        schema_version: 1,
        packet_type: "repo_resume_context".to_string(),
        repo_root: "/repo".to_string(),
        generated_at: "2026-05-08T00:00:00Z".to_string(),
        context_state: ResumeContextState {
            token_estimate: 0,
            token_cap: 2_000,
            truncation_applied: false,
            omitted_count: 0,
            source_stores: vec![
                "git".to_string(),
                "operations".to_string(),
                "overlay".to_string(),
                "context_metrics".to_string(),
            ],
            metrics_hint: Some(super::types::MetricsHint {
                cards_served_total: 1,
                estimated_tokens_saved_total: 2,
                compact_outputs_total: 3,
                resume_context_responses_total: 4,
            }),
        },
        sections: ResumeContextSections {
            changed_files: ChangedFilesSection {
                source_store: "git".to_string(),
                count: 10,
                files: (0..10).map(|idx| format!("src/file_{idx}.rs")).collect(),
            },
            next_actions: NextActionsSection {
                source_store: "repair+overlay+git".to_string(),
                count: 2,
                items: vec![
                    HandoffItem::new(
                        "handoff-1".to_string(),
                        HandoffSource::Repair,
                        "src/lib.rs".to_string(),
                        "Review repair output before release".repeat(20),
                        HandoffPriority::High,
                        ".synrepo/state/repair-log.jsonl".to_string(),
                        None,
                    ),
                    HandoffItem::new(
                        "handoff-2".to_string(),
                        HandoffSource::Hotspot,
                        "src/main.rs".to_string(),
                        "Check recent hotspot churn".repeat(20),
                        HandoffPriority::Medium,
                        "src/main.rs".to_string(),
                        Some(7),
                    ),
                ],
            },
            recent_activity: RecentActivitySection {
                source_store: "operations".to_string(),
                count: 2,
                activity: vec![
                    ActivityEntry {
                        kind: "repair".to_string(),
                        timestamp: "2026-05-08T00:00:00Z".to_string(),
                        payload: json!({"summary": "repair activity".repeat(20)}),
                    },
                    ActivityEntry {
                        kind: "hotspot".to_string(),
                        timestamp: String::new(),
                        payload: json!({"path": "src/lib.rs", "summary": "hotspot".repeat(20)}),
                    },
                ],
            },
            saved_notes: SavedNotesSection {
                source_store: "overlay".to_string(),
                advisory: true,
                overlay_state: "available".to_string(),
                overlay_error: None,
                counts: None,
                count: 2,
                notes: vec![
                    note_summary("note_1", "src/lib.rs", "note claim".repeat(20)),
                    note_summary("note_2", "src/main.rs", "another claim".repeat(20)),
                ],
            },
            validation: ValidationSection {
                recommended_commands: vec![
                    "synrepo status --recent".to_string(),
                    "git status --short".to_string(),
                ],
            },
        },
        detail_pointers: vec![super::types::DetailPointer {
            label: "changed files".to_string(),
            mcp: "synrepo_changed".to_string(),
            cli: "git status --short".to_string(),
        }],
        omitted: Vec::new(),
    }
}

fn note_summary(id: &str, target: &str, claim_preview: String) -> AgentNoteSummary {
    AgentNoteSummary {
        note_id: id.to_string(),
        target_kind: "path".to_string(),
        target: target.to_string(),
        status: "active".to_string(),
        confidence: "medium".to_string(),
        updated_at: "2026-05-08T00:00:00Z".to_string(),
        claim_preview,
        source_store: "overlay".to_string(),
        advisory: true,
    }
}
