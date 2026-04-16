use crate::pipeline::repair::{
    DriftClass, RepairAction, RepairFinding, RepairSurface, Severity,
};

use super::{RepairContext, SurfaceCheck};

pub(super) struct CommentaryScan {
    total: usize,
    stale: usize,
}

fn scan_commentary_staleness(synrepo_dir: &std::path::Path) -> crate::Result<CommentaryScan> {
    use crate::core::ids::NodeId;
    use crate::pipeline::repair::commentary::resolve_commentary_node;
    use crate::store::overlay::SqliteOverlayStore;
    use crate::store::sqlite::SqliteGraphStore;
    use std::str::FromStr;

    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    let rows = overlay.commentary_hashes()?;

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;

    let mut total = 0usize;
    let mut stale = 0usize;
    for (node_id_str, stored_hash) in rows {
        total += 1;
        let Ok(node_id) = NodeId::from_str(&node_id_str) else {
            stale += 1;
            continue;
        };
        let fresh = resolve_commentary_node(&graph, node_id)?
            .is_some_and(|snap| snap.content_hash == stored_hash);
        if !fresh {
            stale += 1;
        }
    }

    Ok(CommentaryScan { total, stale })
}

pub struct CommentaryOverlayCheck;

impl SurfaceCheck for CommentaryOverlayCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::CommentaryOverlayEntries
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        use crate::store::overlay::SqliteOverlayStore;

        let overlay_dir = ctx.synrepo_dir.join("overlay");
        let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
        if !overlay_db.exists() {
            return vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Absent,
                severity: Severity::ReportOnly,
                target_id: None,
                recommended_action: RepairAction::None,
                notes: Some(
                    "Commentary overlay has not been materialized yet (no overlay.db).".to_string(),
                ),
            }];
        }

        match scan_commentary_staleness(ctx.synrepo_dir) {
            Ok(CommentaryScan { total, stale }) if stale > 0 => vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Stale,
                severity: Severity::Actionable,
                target_id: None,
                recommended_action: RepairAction::RefreshCommentary,
                notes: Some(format!(
                    "{stale} of {total} commentary entries are stale against the current graph."
                )),
            }],
            Ok(CommentaryScan { total, .. }) => vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Current,
                severity: Severity::Actionable,
                target_id: None,
                recommended_action: RepairAction::None,
                notes: Some(format!("{total} commentary entries are current.")),
            }],
            Err(err) => vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Blocked,
                severity: Severity::Blocked,
                target_id: None,
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!("Cannot evaluate commentary staleness: {err}")),
            }],
        }
    }
}
