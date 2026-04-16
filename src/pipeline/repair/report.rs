use std::path::Path;

use crate::{
    config::Config,
    pipeline::{
        export::load_manifest,
        maintenance::{plan_maintenance, MaintenancePlan},
        watch::ReconcileState,
    },
};

use super::{
    declared_links::check_declared_links, DriftClass, RepairAction, RepairFinding, RepairReport,
    RepairSurface, Severity,
};
use crate::pipeline::{
    diagnostics::{collect_diagnostics, ReconcileHealth, WriterStatus},
    writer::now_rfc3339,
};

/// Build a repair report by composing existing diagnostics and maintenance
/// planning. This is the read-only `check` path: no state is mutated.
pub fn build_repair_report(synrepo_dir: &Path, config: &Config) -> RepairReport {
    let maint_plan = plan_maintenance(synrepo_dir, config);
    assemble_repair_report(synrepo_dir, config, &maint_plan)
}

/// Inner report builder. Accepts a pre-computed `maint_plan` so callers that
/// also need to execute maintenance (e.g. `execute_sync`) don't call
/// `plan_maintenance` twice.
pub(super) fn assemble_repair_report(
    synrepo_dir: &Path,
    config: &Config,
    maint_plan: &crate::Result<MaintenancePlan>,
) -> RepairReport {
    let now = now_rfc3339();
    let mut findings = Vec::new();
    let diag = collect_diagnostics(synrepo_dir, config);

    let repo_root = synrepo_dir.parent().unwrap_or(synrepo_dir);
    findings.push(writer_lock_finding(&diag.writer_status));
    findings.push(store_maintenance_finding(maint_plan));
    findings.push(structural_refresh_finding(&diag.reconcile_health));
    findings.push(check_declared_links(synrepo_dir));
    findings.push(commentary_overlay_finding(synrepo_dir));
    findings.push(export_surface_finding(
        repo_root,
        config,
        diag.last_reconcile.as_ref(),
    ));
    findings.extend(edge_drift_findings(synrepo_dir).unwrap_or_default());
    findings.extend(proposed_links_overlay_findings(synrepo_dir));
    findings.extend(unsupported_surface_findings());

    RepairReport {
        checked_at: now,
        findings,
    }
}

fn writer_lock_finding(status: &WriterStatus) -> RepairFinding {
    match status {
        WriterStatus::HeldByOther { pid } => RepairFinding {
            surface: RepairSurface::WriterLock,
            drift_class: DriftClass::Blocked,
            severity: Severity::Blocked,
            target_id: Some(pid.to_string()),
            recommended_action: RepairAction::ManualReview,
            notes: Some(format!(
                "Writer lock held by pid {pid}. Verify the process is alive before removing the lock."
            )),
        },
        WriterStatus::Free | WriterStatus::HeldBySelf => RepairFinding {
            surface: RepairSurface::WriterLock,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: None,
        },
    }
}

fn store_maintenance_finding(maint_plan: &crate::Result<MaintenancePlan>) -> RepairFinding {
    match maint_plan {
        Ok(plan) if plan.has_work() => {
            let store_names: Vec<String> = plan
                .pending_actions()
                .map(|a| a.store_id.as_str().to_string())
                .collect();
            RepairFinding {
                surface: RepairSurface::StoreMaintenance,
                drift_class: DriftClass::Stale,
                severity: Severity::Actionable,
                target_id: None,
                recommended_action: RepairAction::RunMaintenance,
                notes: Some(format!(
                    "Stores needing maintenance: {}",
                    store_names.join(", ")
                )),
            }
        }
        Ok(_) => RepairFinding {
            surface: RepairSurface::StoreMaintenance,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: None,
        },
        Err(err) => RepairFinding {
            surface: RepairSurface::StoreMaintenance,
            drift_class: DriftClass::Blocked,
            severity: Severity::Blocked,
            target_id: None,
            recommended_action: RepairAction::ManualReview,
            notes: Some(format!("Cannot evaluate storage: {err}")),
        },
    }
}

fn structural_refresh_finding(health: &ReconcileHealth) -> RepairFinding {
    let (drift, severity, action, notes) = match health {
        ReconcileHealth::Current => (
            DriftClass::Current,
            Severity::Actionable,
            RepairAction::None,
            None,
        ),
        ReconcileHealth::Unknown => (
            DriftClass::Absent,
            Severity::Actionable,
            RepairAction::RunReconcile,
            Some(
                "Graph has never been populated. Run `synrepo reconcile` or `synrepo init`."
                    .to_string(),
            ),
        ),
        ReconcileHealth::Stale { last_outcome } => (
            DriftClass::Stale,
            Severity::Actionable,
            RepairAction::RunReconcile,
            Some(format!("Last reconcile outcome: {last_outcome}")),
        ),
    };

    RepairFinding {
        surface: RepairSurface::StructuralRefresh,
        drift_class: drift,
        severity,
        target_id: None,
        recommended_action: action,
        notes,
    }
}

fn unsupported_surface_findings() -> [RepairFinding; 1] {
    [(
        RepairSurface::StaleRationale,
        "Rationale drift scoring is not yet implemented.",
    )]
    .map(|(surface, hint)| RepairFinding {
        surface,
        drift_class: DriftClass::Unsupported,
        severity: Severity::Unsupported,
        target_id: None,
        recommended_action: RepairAction::NotSupported,
        notes: Some(hint.to_string()),
    })
}

/// Check export surface freshness against the current reconcile epoch.
///
/// - No manifest → `Absent`, actionable, `RegenerateExports`.
/// - Manifest epoch behind current `last_reconcile_at` → `Stale`, actionable.
/// - Manifest epoch matches current → `Current`.
fn export_surface_finding(
    repo_root: &Path,
    config: &Config,
    last_reconcile: Option<&ReconcileState>,
) -> RepairFinding {
    let manifest = load_manifest(repo_root, config);

    match manifest {
        None => RepairFinding {
            surface: RepairSurface::ExportSurface,
            drift_class: DriftClass::Absent,
            severity: Severity::ReportOnly,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some(
                "Export directory has not been generated yet. Run `synrepo export`.".to_string(),
            ),
        },
        Some(manifest) => {
            let current_epoch = last_reconcile
                .map(|r| r.last_reconcile_at.as_str())
                .unwrap_or_default();
            if manifest.last_reconcile_at == current_epoch {
                RepairFinding {
                    surface: RepairSurface::ExportSurface,
                    drift_class: DriftClass::Current,
                    severity: Severity::Actionable,
                    target_id: None,
                    recommended_action: RepairAction::None,
                    notes: None,
                }
            } else {
                RepairFinding {
                    surface: RepairSurface::ExportSurface,
                    drift_class: DriftClass::Stale,
                    severity: Severity::Actionable,
                    target_id: None,
                    recommended_action: RepairAction::RegenerateExports,
                    notes: Some(format!(
                        "Export was generated at reconcile epoch `{}`, but current epoch is `{}`.",
                        manifest.last_reconcile_at, current_epoch
                    )),
                }
            }
        }
    }
}

/// Classify the commentary overlay surface.
///
/// - No `overlay.db` on disk → `DriftClass::Absent`, no action required.
/// - Overlay present, staleness sweep runs: if any stored commentary entry
///   references a content hash that no longer matches the current graph file
///   → `DriftClass::Stale` with a `RefreshCommentary` recommendation.
/// - Overlay present with zero entries or all entries fresh → `DriftClass::Current`.
/// - If the graph is not available or the scan errors out, the finding is
///   reported as blocked so callers can distinguish "no drift" from
///   "couldn't evaluate."
fn commentary_overlay_finding(synrepo_dir: &Path) -> RepairFinding {
    use crate::store::overlay::SqliteOverlayStore;

    let overlay_dir = synrepo_dir.join("overlay");
    let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
    if !overlay_db.exists() {
        return RepairFinding {
            surface: RepairSurface::CommentaryOverlayEntries,
            drift_class: DriftClass::Absent,
            severity: Severity::ReportOnly,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some(
                "Commentary overlay has not been materialized yet (no overlay.db).".to_string(),
            ),
        };
    }

    match scan_commentary_staleness(synrepo_dir) {
        Ok(CommentaryScan { total, stale }) if stale > 0 => RepairFinding {
            surface: RepairSurface::CommentaryOverlayEntries,
            drift_class: DriftClass::Stale,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::RefreshCommentary,
            notes: Some(format!(
                "{stale} of {total} commentary entries are stale against the current graph."
            )),
        },
        Ok(CommentaryScan { total, .. }) => RepairFinding {
            surface: RepairSurface::CommentaryOverlayEntries,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some(format!("{total} commentary entries are current.")),
        },
        Err(err) => RepairFinding {
            surface: RepairSurface::CommentaryOverlayEntries,
            drift_class: DriftClass::Blocked,
            severity: Severity::Blocked,
            target_id: None,
            recommended_action: RepairAction::ManualReview,
            notes: Some(format!("Cannot evaluate commentary staleness: {err}")),
        },
    }
}

struct CommentaryScan {
    total: usize,
    stale: usize,
}

/// Build findings for the cross-link overlay surface.
///
/// - No `overlay.db` or empty `cross_links` table → one `Absent` report-only
///   finding.
/// - Each stale row (current content hash differs from stored) produces one
///   `Stale` / `RevalidateLinks` finding.
/// - Each source-deleted row (either endpoint absent from the graph or backing
///   file missing) produces one `SourceDeleted` / `ManualReview` finding.
/// - All rows fresh → one `Current` finding carrying the total count.
/// - Scan errors → one `Blocked` finding.
fn proposed_links_overlay_findings(synrepo_dir: &Path) -> Vec<RepairFinding> {
    use crate::store::overlay::SqliteOverlayStore;

    let overlay_dir = synrepo_dir.join("overlay");
    let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
    if !overlay_db.exists() {
        return vec![absent_cross_link_finding(
            "Cross-link overlay has not been materialized yet (no overlay.db).",
        )];
    }

    match scan_cross_links(synrepo_dir) {
        Ok(scan) => {
            if scan.total == 0 {
                return vec![absent_cross_link_finding(
                    "Cross-link overlay is empty; no candidates to evaluate.",
                )];
            }
            let mut out = Vec::new();
            for row in scan.drifted {
                out.push(drifted_cross_link_finding(row));
            }
            if out.is_empty() {
                out.push(RepairFinding {
                    surface: RepairSurface::ProposedLinksOverlay,
                    drift_class: DriftClass::Current,
                    severity: Severity::Actionable,
                    target_id: None,
                    recommended_action: RepairAction::None,
                    notes: Some(format!("{} cross-link candidates are current.", scan.total)),
                });
            }
            out
        }
        Err(err) => vec![RepairFinding {
            surface: RepairSurface::ProposedLinksOverlay,
            drift_class: DriftClass::Blocked,
            severity: Severity::Blocked,
            target_id: None,
            recommended_action: RepairAction::ManualReview,
            notes: Some(format!("Cannot evaluate cross-link staleness: {err}")),
        }],
    }
}

fn absent_cross_link_finding(note: &str) -> RepairFinding {
    RepairFinding {
        surface: RepairSurface::ProposedLinksOverlay,
        drift_class: DriftClass::Absent,
        severity: Severity::ReportOnly,
        target_id: None,
        recommended_action: RepairAction::None,
        notes: Some(note.to_string()),
    }
}

fn drifted_cross_link_finding(row: DriftedCrossLink) -> RepairFinding {
    let target = format!(
        "from={} to={} kind={}",
        row.from_node, row.to_node, row.kind
    );
    match row.classification {
        CrossLinkDrift::Stale => RepairFinding {
            surface: RepairSurface::ProposedLinksOverlay,
            drift_class: DriftClass::Stale,
            severity: Severity::Actionable,
            target_id: Some(target),
            recommended_action: RepairAction::RevalidateLinks,
            notes: Some(
                "Stored endpoint hash no longer matches the current graph content.".to_string(),
            ),
        },
        CrossLinkDrift::SourceDeleted => RepairFinding {
            surface: RepairSurface::ProposedLinksOverlay,
            drift_class: DriftClass::SourceDeleted,
            severity: Severity::ReportOnly,
            target_id: Some(target),
            recommended_action: RepairAction::ManualReview,
            notes: Some("One or both endpoints are absent from the current graph.".to_string()),
        },
    }
}

enum CrossLinkDrift {
    Stale,
    SourceDeleted,
}

struct DriftedCrossLink {
    from_node: String,
    to_node: String,
    kind: String,
    classification: CrossLinkDrift,
}

/// Build findings for edges with high drift scores.
fn edge_drift_findings(synrepo_dir: &Path) -> crate::Result<Vec<RepairFinding>> {
    use crate::store::sqlite::SqliteGraphStore;
    use crate::structure::graph::GraphStore;

    let graph_dir = synrepo_dir.join("graph");
    let Ok(graph) = SqliteGraphStore::open_existing(&graph_dir) else {
        return Ok(vec![RepairFinding {
            surface: RepairSurface::EdgeDrift,
            drift_class: DriftClass::Absent,
            severity: Severity::Unsupported,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some("graph store not materialized; edge drift check skipped".to_string()),
        }]);
    };

    // Use the latest revision actually recorded in edge_drift rather than
    // attempting to infer it from file provenance. File provenance revisions
    // reflect when each file was *last parsed*, not the current pipeline run,
    // so a find_map across files would grab an arbitrarily stale revision for
    // unchanged files — causing read_drift_scores to return 0 rows because the
    // compiler truncates scores older than the current revision.
    let Some(revision) = graph.latest_drift_revision()? else {
        return Ok(vec![RepairFinding {
            surface: RepairSurface::EdgeDrift,
            drift_class: DriftClass::Absent,
            severity: Severity::Unsupported,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some(
                "no drift assessment performed yet; run a structural compile first".to_string(),
            ),
        }]);
    };

    let scores = match graph.read_drift_scores(&revision) {
        Ok(scores) => scores,
        Err(_) => return Ok(Vec::new()),
    };

    if scores.is_empty() {
        return Ok(vec![RepairFinding {
            surface: RepairSurface::EdgeDrift,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some("no drifted edges detected".to_string()),
        }]);
    }

    let high_drift: Vec<_> = scores.iter().filter(|(_, score)| *score >= 0.7).collect();
    let dead_edges: Vec<_> = scores
        .iter()
        .filter(|(_, score)| (*score - 1.0).abs() < f32::EPSILON)
        .collect();

    let mut findings = Vec::new();

    if !high_drift.is_empty() {
        findings.push(RepairFinding {
            surface: RepairSurface::EdgeDrift,
            drift_class: DriftClass::HighDriftEdge,
            severity: if dead_edges.is_empty() {
                Severity::ReportOnly
            } else {
                Severity::Actionable
            },
            target_id: None,
            recommended_action: if dead_edges.is_empty() {
                RepairAction::ManualReview
            } else {
                RepairAction::RunReconcile
            },
            notes: Some(format!(
                "{} edges at drift >= 0.7 ({} at 1.0, prunable)",
                high_drift.len(),
                dead_edges.len(),
            )),
        });
    }

    Ok(findings)
}

struct CrossLinkScan {
    total: usize,
    drifted: Vec<DriftedCrossLink>,
}

fn scan_cross_links(synrepo_dir: &Path) -> crate::Result<CrossLinkScan> {
    use crate::core::ids::NodeId;
    use crate::store::overlay::SqliteOverlayStore;
    use crate::store::sqlite::SqliteGraphStore;
    use std::str::FromStr;

    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    let rows = overlay.cross_link_hashes()?;
    if rows.is_empty() {
        return Ok(CrossLinkScan {
            total: 0,
            drifted: Vec::new(),
        });
    }

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;

    let mut drifted = Vec::new();
    let total = rows.len();
    for row in rows {
        let from = NodeId::from_str(&row.from_node);
        let to = NodeId::from_str(&row.to_node);
        let (Ok(from), Ok(to)) = (from, to) else {
            drifted.push(DriftedCrossLink {
                from_node: row.from_node,
                to_node: row.to_node,
                kind: row.kind,
                classification: CrossLinkDrift::SourceDeleted,
            });
            continue;
        };

        let current_from = current_endpoint_hash(&graph, from)?;
        let current_to = current_endpoint_hash(&graph, to)?;

        let classification = match (current_from, current_to) {
            (Some(f), Some(t)) if f == row.from_content_hash && t == row.to_content_hash => {
                continue;
            }
            (Some(_), Some(_)) => CrossLinkDrift::Stale,
            _ => CrossLinkDrift::SourceDeleted,
        };
        drifted.push(DriftedCrossLink {
            from_node: row.from_node,
            to_node: row.to_node,
            kind: row.kind,
            classification,
        });
    }

    Ok(CrossLinkScan { total, drifted })
}

/// Resolve the current content hash for a cross-link endpoint. `None` means
/// the node (or its backing file, for concepts and symbols) is no longer in
/// the graph.
fn current_endpoint_hash(
    graph: &crate::store::sqlite::SqliteGraphStore,
    node: crate::core::ids::NodeId,
) -> crate::Result<Option<String>> {
    use crate::core::ids::NodeId;
    use crate::structure::graph::GraphStore;

    match node {
        NodeId::File(file_id) => Ok(graph.get_file(file_id)?.map(|f| f.content_hash)),
        NodeId::Symbol(sym_id) => {
            let Some(sym) = graph.get_symbol(sym_id)? else {
                return Ok(None);
            };
            Ok(graph.get_file(sym.file_id)?.map(|f| f.content_hash))
        }
        NodeId::Concept(concept_id) => {
            let Some(concept) = graph.get_concept(concept_id)? else {
                return Ok(None);
            };
            Ok(graph.file_by_path(&concept.path)?.map(|f| f.content_hash))
        }
    }
}

/// Walk every row in the `commentary` table and compare its stored
/// `source_content_hash` against the graph's current content hash for the
/// referenced node. A mismatch counts as stale; a missing node counts as
/// stale too (the pruner will eventually remove it, but until then it still
/// points at an out-of-date snapshot).
fn scan_commentary_staleness(synrepo_dir: &Path) -> crate::Result<CommentaryScan> {
    use super::commentary::resolve_commentary_node;
    use crate::core::ids::NodeId;
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
