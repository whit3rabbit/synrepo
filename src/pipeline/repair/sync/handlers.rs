//! Action handlers extracted from sync.rs.
//!
//! These handle the various repair actions for auto-fixable findings.

use std::path::Path;
use std::str::FromStr;

use anyhow::anyhow;

use crate::{
    config::Config,
    core::ids::NodeId,
    overlay::{CommentaryProvenance, OverlayStore},
    pipeline::{
        maintenance::execute_maintenance,
        structural::run_structural_compile,
        synthesis::{build_commentary_generator, CommentaryGenerator},
        watch::{
            emit_cochange_edges_pass, emit_symbol_revisions_pass, persist_reconcile_state,
            ReconcileOutcome,
        },
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::GraphStore,
};

use crate::pipeline::repair::{DriftClass, RepairAction, RepairFinding, RepairSurface, Severity};

/// Context for action handlers.
pub struct ActionContext<'a> {
    /// Repository root (absolute path).
    pub repo_root: &'a Path,
    /// `.synrepo/` directory for the repo.
    pub synrepo_dir: &'a Path,
    /// Loaded runtime config.
    pub config: &'a Config,
    /// Pre-computed maintenance plan shared across repair actions.
    pub maint_plan: &'a crate::Result<crate::pipeline::maintenance::MaintenancePlan>,
}

/// Run maintenance if the plan has work.
pub fn run_maintenance_if_needed(
    synrepo_dir: &Path,
    maint_plan: &crate::Result<crate::pipeline::maintenance::MaintenancePlan>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    let plan = maint_plan.as_ref().map_err(|e| anyhow!("{e}"))?;
    if plan.has_work() {
        execute_maintenance(synrepo_dir, plan)?;
        actions_taken.push("ran maintenance".to_string());
    }
    Ok(())
}

/// Handle actionable finding based on recommended action.
pub fn handle_actionable_finding(
    finding: &RepairFinding,
    context: &ActionContext<'_>,
    repaired: &mut Vec<RepairFinding>,
    report_only: &mut Vec<RepairFinding>,
    blocked: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    match finding.recommended_action {
        RepairAction::None => {}
        RepairAction::RunMaintenance => {
            run_maintenance_if_needed(context.synrepo_dir, context.maint_plan, actions_taken)?;
            repaired.push(finding.clone());
        }
        RepairAction::RunMaintenanceThenReconcile => {
            run_maintenance_if_needed(context.synrepo_dir, context.maint_plan, actions_taken)?;
            record_reconcile_attempt(
                finding,
                context.repo_root,
                context.synrepo_dir,
                context.config,
                repaired,
                blocked,
                actions_taken,
            );
        }
        RepairAction::RunReconcile => {
            if finding.surface == RepairSurface::EdgeDrift {
                prune_dead_edges(finding, context.synrepo_dir, repaired, actions_taken)?;
            } else {
                record_reconcile_attempt(
                    finding,
                    context.repo_root,
                    context.synrepo_dir,
                    context.config,
                    repaired,
                    blocked,
                    actions_taken,
                );
            }
        }
        RepairAction::ManualReview | RepairAction::NotSupported => {
            report_only.push(finding.clone());
        }
        RepairAction::RevalidateLinks => {
            // Revalidation deferred — the fuzzy-LCS verifier is not yet wired.
            // Stale candidates remain on disk with their tier intact.
            actions_taken.push(format!(
                "cross-link revalidation deferred for {}: verifier not yet wired",
                finding.surface.as_str()
            ));
            report_only.push(finding.clone());
        }
        RepairAction::RegenerateExports => match regenerate_exports(context, actions_taken) {
            Ok(()) => repaired.push(finding.clone()),
            Err(err) => {
                actions_taken.push(format!(
                    "export regeneration failed for {}: {err}",
                    finding.surface.as_str()
                ));
                let mut blocked_finding = finding.clone();
                blocked_finding.drift_class = DriftClass::Blocked;
                blocked_finding.severity = Severity::Blocked;
                blocked_finding.recommended_action = RepairAction::ManualReview;
                blocked_finding.notes = Some(format!("Export regeneration failed: {err}"));
                blocked.push(blocked_finding);
            }
        },
        RepairAction::CompactRetired => {
            match compact_retired_observations(context, actions_taken) {
                Ok(()) => repaired.push(finding.clone()),
                Err(err) => {
                    actions_taken.push(format!("compaction failed: {err}"));
                    let mut blocked_finding = finding.clone();
                    blocked_finding.drift_class = DriftClass::Blocked;
                    blocked_finding.severity = Severity::Blocked;
                    blocked_finding.recommended_action = RepairAction::ManualReview;
                    blocked_finding.notes = Some(format!("Compaction failed: {err}"));
                    blocked.push(blocked_finding);
                }
            }
        }
        RepairAction::RefreshCommentary => match refresh_commentary(context, actions_taken, None) {
            Ok(()) => repaired.push(finding.clone()),
            Err(err) => {
                actions_taken.push(format!(
                    "commentary refresh failed for {}: {err}",
                    finding.surface.as_str()
                ));
                let mut blocked_finding = finding.clone();
                blocked_finding.drift_class = DriftClass::Blocked;
                blocked_finding.severity = Severity::Blocked;
                blocked_finding.recommended_action = RepairAction::ManualReview;
                blocked_finding.notes = Some(format!("Commentary refresh failed: {err}"));
                blocked.push(blocked_finding);
            }
        },
    }
    Ok(())
}

/// Prune edges with drift score of 1.0 (dead edges).
pub fn prune_dead_edges(
    finding: &RepairFinding,
    synrepo_dir: &Path,
    repaired: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    let graph_dir = synrepo_dir.join("graph");
    let Ok(mut graph) = SqliteGraphStore::open_existing(&graph_dir) else {
        actions_taken.push("edge drift pruning skipped: graph store not found".to_string());
        return Ok(());
    };

    // Use the latest revision recorded in edge_drift.
    let Some(revision) = graph.latest_drift_revision()? else {
        return Ok(());
    };

    let scores = graph.read_drift_scores(&revision)?;
    let dead: Vec<_> = scores
        .iter()
        .filter(|(_, score)| (*score - 1.0).abs() < f32::EPSILON)
        .collect();

    if dead.is_empty() {
        return Ok(());
    }

    let mut pruned = 0;
    for (edge_id, _) in &dead {
        if graph.delete_edge(*edge_id).is_ok() {
            pruned += 1;
        }
    }

    actions_taken.push(format!("pruned {pruned} dead edges (drift 1.0)"));
    repaired.push(finding.clone());
    Ok(())
}

/// Refresh stale commentary entries.
///
/// When `scope` is `Some(paths)`, only files whose path starts with one of the
/// prefixes is considered. Prefixes are repo-root-relative; each is normalized
/// to end in `/` so `src` cannot spuriously match `src-extra/...`.
pub fn refresh_commentary(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
    scope: Option<&[std::path::PathBuf]>,
) -> crate::Result<()> {
    use crate::pipeline::repair::commentary::resolve_commentary_node;

    let overlay_dir = context.synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open_existing(&overlay_dir)?;
    let graph = SqliteGraphStore::open_existing(&context.synrepo_dir.join("graph"))?;
    let generator: Box<dyn CommentaryGenerator> =
        build_commentary_generator(context.config, context.config.commentary_cost_limit);

    // Pre-normalize scope prefixes once so the hot loop only compares strings.
    let scope_prefixes: Option<Vec<String>> = scope.map(normalize_scope_prefixes);

    let rows = overlay.commentary_hashes()?;
    let mut refreshed = 0usize;
    let mut skipped = 0usize;
    let mut out_of_scope = 0usize;

    for (node_id_str, stored_hash) in rows {
        let Ok(node_id) = NodeId::from_str(&node_id_str) else {
            skipped += 1;
            continue;
        };
        let Some(snap) = resolve_commentary_node(&graph, node_id)? else {
            skipped += 1;
            continue;
        };
        if let Some(prefixes) = &scope_prefixes {
            if !path_matches_any_prefix(&snap.file.path, prefixes) {
                out_of_scope += 1;
                continue;
            }
        }
        if snap.content_hash == stored_hash {
            continue; // already fresh
        }

        let ctx_text = match &snap.symbol {
            Some(sym) => format!(
                "Symbol {} in {}\nSignature: {}\nDoc: {}",
                sym.qualified_name,
                snap.file.path,
                sym.signature.clone().unwrap_or_default(),
                sym.doc_comment.clone().unwrap_or_default(),
            ),
            None => format!("File: {}", snap.file.path),
        };

        let Some(mut entry) = generator.generate(node_id, &ctx_text)? else {
            skipped += 1;
            continue;
        };
        entry.provenance = CommentaryProvenance {
            source_content_hash: snap.content_hash,
            ..entry.provenance
        };
        overlay.insert_commentary(entry)?;
        refreshed += 1;
    }

    let scope_note = if scope_prefixes.is_some() {
        format!(", {out_of_scope} outside scope")
    } else {
        String::new()
    };
    actions_taken.push(format!(
        "commentary refresh: {refreshed} regenerated, {skipped} skipped (no hash change or no generator output){scope_note}"
    ));
    Ok(())
}

/// Record a reconcile attempt and persist state.
pub fn record_reconcile_attempt(
    finding: &RepairFinding,
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    repaired: &mut Vec<RepairFinding>,
    blocked: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) {
    let graph_dir = synrepo_dir.join("graph");
    let mut graph = match SqliteGraphStore::open(&graph_dir) {
        Ok(g) => g,
        Err(err) => {
            let message = err.to_string();
            actions_taken.push(format!(
                "structural reconcile for {} failed to open graph: {}",
                finding.surface.as_str(),
                message
            ));
            blocked.push(blocked_reconcile_finding(
                finding,
                format!("Reconcile failed: could not open graph store: {message}"),
            ));
            return;
        }
    };

    let outcome = match run_structural_compile(repo_root, config, &mut graph) {
        Ok(summary) => {
            if let Err(err) = emit_cochange_edges_pass(repo_root, config, &mut graph) {
                tracing::warn!(error = %err, "co-change edge emission failed; continuing");
            }
            if let Err(err) = emit_symbol_revisions_pass(repo_root, config, &mut graph) {
                tracing::warn!(error = %err, "symbol revision derivation failed; continuing");
            }
            if let Err(err) = crate::substrate::build_index(config, repo_root) {
                ReconcileOutcome::Failed(format!("index rebuild failed: {err}"))
            } else {
                ReconcileOutcome::Completed(summary)
            }
        }
        Err(err) => ReconcileOutcome::Failed(err.to_string()),
    };

    persist_reconcile_state(synrepo_dir, &outcome, 0);
    match outcome {
        ReconcileOutcome::Completed(_) => {
            actions_taken.push(format!(
                "ran structural reconcile for {}",
                finding.surface.as_str()
            ));
            repaired.push(finding.clone());
        }
        ReconcileOutcome::LockConflict { holder_pid } => {
            let message =
                format!("unexpected lock conflict with PID {holder_pid} while holding writer lock");
            tracing::error!(%message);
            blocked.push(blocked_reconcile_finding(finding, message));
        }
        ReconcileOutcome::Failed(message) => {
            actions_taken.push(format!(
                "structural reconcile for {} failed: {}",
                finding.surface.as_str(),
                message
            ));
            blocked.push(blocked_reconcile_finding(
                finding,
                format!(
                    "Reconcile failed while repairing {}: {message}",
                    finding.surface.as_str()
                ),
            ));
        }
    }
}

/// Re-run export generation.
pub fn regenerate_exports(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    use crate::pipeline::export::{load_manifest, write_exports, ExportFormat};
    use crate::surface::card::Budget;

    let existing = load_manifest(context.repo_root, context.config);
    let format = existing
        .as_ref()
        .map(|m| m.format)
        .unwrap_or(ExportFormat::Markdown);
    let budget = existing
        .as_ref()
        .and_then(|m| match m.budget.as_str() {
            "deep" => Some(Budget::Deep),
            "normal" => Some(Budget::Normal),
            _ => None,
        })
        .unwrap_or(Budget::Normal);

    write_exports(
        context.repo_root,
        context.synrepo_dir,
        context.config,
        format,
        budget,
        false,
    )
    .map_err(|e| anyhow!("{e}"))?;

    actions_taken.push(format!(
        "regenerated export directory (format={}, budget={})",
        format.as_str(),
        match budget {
            Budget::Tiny => "tiny",
            Budget::Normal => "normal",
            Budget::Deep => "deep",
        }
    ));
    Ok(())
}

/// Run compaction on retired observations.
pub fn compact_retired_observations(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    let graph_dir = context.synrepo_dir.join("graph");
    let mut graph = SqliteGraphStore::open_existing(&graph_dir)?;

    let current_rev = graph.next_compile_revision()?;
    let retain = context.config.retain_retired_revisions;
    if current_rev <= retain {
        actions_taken.push("compaction skipped: not enough revisions yet".to_string());
        return Ok(());
    }
    let threshold = current_rev - retain;
    let summary = graph.compact_retired(threshold)?;

    actions_taken.push(format!(
        "compaction: removed {} retired symbols, {} retired edges, {} old revisions",
        summary.symbols_removed, summary.edges_removed, summary.revisions_removed
    ));
    Ok(())
}

fn blocked_reconcile_finding(finding: &RepairFinding, notes: String) -> RepairFinding {
    let mut blocked = finding.clone();
    blocked.drift_class = DriftClass::Blocked;
    blocked.severity = Severity::Blocked;
    blocked.recommended_action = RepairAction::ManualReview;
    blocked.notes = Some(notes);
    blocked
}

/// Convert scope `PathBuf`s into `/`-normalized, trailing-slash-terminated
/// string prefixes so a prefix-match cannot spuriously accept sibling
/// directories (`src` matching `src-extra/...`).
pub(crate) fn normalize_scope_prefixes(paths: &[std::path::PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| {
            let mut s = p.to_string_lossy().replace('\\', "/");
            if !s.is_empty() && !s.ends_with('/') {
                s.push('/');
            }
            s
        })
        .collect()
}

/// True if `file_path` (stored as recorded in the graph, possibly with
/// backslashes on Windows) starts with any of the normalized prefixes.
pub(crate) fn path_matches_any_prefix(file_path: &str, prefixes: &[String]) -> bool {
    let normalized = file_path.replace('\\', "/");
    prefixes.iter().any(|p| normalized.starts_with(p.as_str()))
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn prefix_is_terminated_with_slash() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("src")]);
        assert_eq!(prefixes, vec!["src/".to_string()]);
    }

    #[test]
    fn prefix_sibling_does_not_match() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("src")]);
        assert!(path_matches_any_prefix("src/lib.rs", &prefixes));
        assert!(!path_matches_any_prefix("src-extra/lib.rs", &prefixes));
    }

    #[test]
    fn backslash_paths_match_forward_slash_prefix() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("src")]);
        assert!(path_matches_any_prefix("src\\lib.rs", &prefixes));
    }

    #[test]
    fn empty_scope_matches_nothing() {
        let prefixes = normalize_scope_prefixes(&[]);
        assert!(!path_matches_any_prefix("src/lib.rs", &prefixes));
    }

    #[test]
    fn nested_prefix_match() {
        let prefixes = normalize_scope_prefixes(&[PathBuf::from("crates/core/src")]);
        assert!(path_matches_any_prefix("crates/core/src/lib.rs", &prefixes));
        assert!(!path_matches_any_prefix(
            "crates/core/tests/a.rs",
            &prefixes
        ));
    }
}
