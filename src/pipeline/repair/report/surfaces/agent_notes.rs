use crate::overlay::AgentNoteStatus;
use crate::pipeline::repair::{DriftClass, RepairAction, RepairFinding, RepairSurface, Severity};

use super::{RepairContext, SurfaceCheck};

pub struct AgentNotesOverlayCheck;

impl SurfaceCheck for AgentNotesOverlayCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::AgentNotesOverlay
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        use crate::store::overlay::{current_drifted_note_ids, SqliteOverlayStore};
        use crate::store::sqlite::SqliteGraphStore;

        let overlay_dir = ctx.synrepo_dir.join("overlay");
        let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
        if !overlay_db.exists() {
            return vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Absent,
                severity: Severity::ReportOnly,
                target_id: None,
                recommended_action: RepairAction::None,
                notes: Some("Agent-note overlay has not been materialized yet.".to_string()),
            }];
        }

        let overlay = match SqliteOverlayStore::open_existing(&overlay_dir) {
            Ok(store) => store,
            Err(err) => {
                return vec![RepairFinding {
                    surface: self.surface(),
                    drift_class: DriftClass::Blocked,
                    severity: Severity::Blocked,
                    target_id: None,
                    recommended_action: RepairAction::ManualReview,
                    notes: Some(format!("Cannot open agent-note overlay: {err}")),
                }]
            }
        };
        let notes = match overlay.all_notes() {
            Ok(notes) => notes,
            Err(err) => {
                return vec![RepairFinding {
                    surface: self.surface(),
                    drift_class: DriftClass::Blocked,
                    severity: Severity::Blocked,
                    target_id: None,
                    recommended_action: RepairAction::ManualReview,
                    notes: Some(format!("Cannot read agent notes: {err}")),
                }]
            }
        };
        if notes.is_empty() {
            return vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Current,
                severity: Severity::Actionable,
                target_id: None,
                recommended_action: RepairAction::None,
                notes: Some("Agent-note overlay is empty.".to_string()),
            }];
        }

        let mut findings = Vec::new();
        for note in &notes {
            if note.status == AgentNoteStatus::Invalid {
                findings.push(RepairFinding {
                    surface: self.surface(),
                    drift_class: DriftClass::Corrupted,
                    severity: Severity::Actionable,
                    target_id: Some(note.note_id.clone()),
                    recommended_action: RepairAction::ManualReview,
                    notes: Some(
                        "Invalid advisory note; reverify, supersede, or forget it.".to_string(),
                    ),
                });
            } else if note.status == AgentNoteStatus::Stale {
                findings.push(RepairFinding {
                    surface: self.surface(),
                    drift_class: DriftClass::Stale,
                    severity: Severity::Actionable,
                    target_id: Some(note.note_id.clone()),
                    recommended_action: RepairAction::ManualReview,
                    notes: Some("Stale advisory note; reverify or forget it.".to_string()),
                });
            }
        }

        if let Ok(graph) = SqliteGraphStore::open_existing(&ctx.synrepo_dir.join("graph")) {
            match current_drifted_note_ids(&notes, &graph, None) {
                Ok(ids) => {
                    for note_id in ids {
                        findings.push(RepairFinding {
                            surface: self.surface(),
                            drift_class: DriftClass::Stale,
                            severity: Severity::Actionable,
                            target_id: Some(note_id),
                            recommended_action: RepairAction::RevalidateAgentNotes,
                            notes: Some(
                                "Note drift anchors no longer match current graph facts."
                                    .to_string(),
                            ),
                        });
                    }
                }
                Err(err) => findings.push(RepairFinding {
                    surface: self.surface(),
                    drift_class: DriftClass::Blocked,
                    severity: Severity::Blocked,
                    target_id: None,
                    recommended_action: RepairAction::ManualReview,
                    notes: Some(format!("Cannot evaluate agent-note drift: {err}")),
                }),
            }
        }

        if findings.is_empty() {
            let counts = overlay.note_counts_impl().unwrap_or_default();
            findings.push(RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Current,
                severity: Severity::Actionable,
                target_id: None,
                recommended_action: RepairAction::None,
                notes: Some(format!(
                    "{} notes current ({} active, {} unverified, {} forgotten).",
                    counts.total(),
                    counts.active,
                    counts.unverified,
                    counts.forgotten
                )),
            });
        }
        findings
    }
}
