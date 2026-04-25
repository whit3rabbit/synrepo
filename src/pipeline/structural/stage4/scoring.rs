use std::collections::HashSet;

use super::context::{CrossFilePending, NameIndex, SymbolMeta, SymbolMetaMap};
use crate::{
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    pipeline::structural::ids::derive_edge_id,
    pipeline::structural::provenance::make_provenance,
    structure::{
        graph::{Edge, EdgeKind, Epistemic, GraphStore, SymbolKind, Visibility},
        parse::ExtractedCallRef,
    },
};

// Scoring weights and cutoffs (see design.md D2). Keep in one place so the
// tests (and the `top_score >= TIE_EMIT_CUTOFF` branch) can reference them.
const SAME_FILE_BONUS: i32 = 100;
const IMPORTED_FILE_BONUS: i32 = 50;
const PUBLIC_BONUS: i32 = 20;
const CRATE_BONUS: i32 = 10;
const PRIVATE_CROSS_FILE_PENALTY: i32 = -100;
const KIND_MATCH_BONUS: i32 = 30;
const PREFIX_MATCH_BONUS: i32 = 40;
/// Minimum score a tied top-candidate group needs before we emit an edge to
/// every member of the tie. Lone winners bypass this and only need score > 0.
const TIE_EMIT_CUTOFF: i32 = IMPORTED_FILE_BONUS;

#[derive(Default)]
pub(super) struct CallStats {
    pub(super) calls_resolved_uniquely: usize,
    pub(super) calls_resolved_ambiguously: usize,
    pub(super) calls_dropped_weak: usize,
    pub(super) calls_dropped_no_candidates: usize,
}

impl CallStats {
    pub(super) fn emitted_edges(&self) -> usize {
        self.calls_resolved_uniquely + self.calls_resolved_ambiguously
    }
}

pub(super) fn emit_calls_for_file(
    graph: &mut dyn GraphStore,
    name_index: &NameIndex,
    symbol_meta: &SymbolMetaMap,
    item: &CrossFilePending,
    imports: &HashSet<FileNodeId>,
    revision: &str,
    scored: &mut Vec<(SymbolNodeId, i32)>,
) -> crate::Result<CallStats> {
    let mut stats = CallStats::default();

    for call_ref in &item.call_refs {
        let candidates = name_index
            .get(&call_ref.callee_name)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        // Track calls with no name-index matches.
        if candidates.is_empty() {
            stats.calls_dropped_no_candidates += 1;
            continue;
        }

        scored.clear();
        scored.extend(candidates.iter().filter_map(|callee_id| {
            symbol_meta.get(callee_id).map(|meta| {
                (
                    *callee_id,
                    score_candidate(call_ref, meta, item.file_id, imports),
                )
            })
        }));

        let Some(&(_, top_score)) = scored.iter().max_by_key(|(_, s)| *s) else {
            stats.calls_dropped_no_candidates += 1;
            continue;
        };
        if top_score <= 0 {
            tracing::debug!(
                call_site = %call_ref.callee_name,
                file = %item.file_path,
                "call dropped: all candidates scored <= 0"
            );
            stats.calls_dropped_weak += 1;
            continue;
        }
        let tie_count = scored.iter().filter(|(_, s)| *s == top_score).count();
        if tie_count > 1 && top_score < TIE_EMIT_CUTOFF {
            tracing::debug!(
                call_site = %call_ref.callee_name,
                file = %item.file_path,
                top_score,
                tie_count,
                "call dropped: ambiguous at low score"
            );
            stats.calls_dropped_weak += 1;
            continue;
        }

        // We have a winner (unique or tied at high score).
        if tie_count > 1 {
            tracing::debug!(
                call_site = %call_ref.callee_name,
                file = %item.file_path,
                top_score,
                tie_count,
                "call resolved: tie-emit at high score"
            );
            stats.calls_resolved_ambiguously += tie_count;
        } else {
            stats.calls_resolved_uniquely += 1;
        }

        for (callee_id, s) in scored.iter() {
            if *s != top_score {
                continue;
            }
            graph.insert_edge(build_calls_edge(
                item.file_id,
                *callee_id,
                revision,
                &item.file_path,
            ))?;
        }
    }

    // Per-file telemetry rollup.
    tracing::trace!(
        file = %item.file_path,
        calls_resolved_uniquely = stats.calls_resolved_uniquely,
        calls_resolved_ambiguously = stats.calls_resolved_ambiguously,
        calls_dropped_weak = stats.calls_dropped_weak,
        calls_dropped_no_candidates = stats.calls_dropped_no_candidates,
        "stage4 call-resolution summary"
    );

    Ok(stats)
}

/// Score a candidate symbol for call resolution per design.md D2 (scoring rubric
/// documented next to the constants).
fn score_candidate(
    call_ref: &ExtractedCallRef,
    candidate: &SymbolMeta,
    importing_file_id: FileNodeId,
    imports: &HashSet<FileNodeId>,
) -> i32 {
    let mut score = 0;
    let same_file = candidate.file_id == importing_file_id;

    if same_file {
        score += SAME_FILE_BONUS;
    } else if imports.contains(&candidate.file_id) {
        score += IMPORTED_FILE_BONUS;
    }

    match candidate.visibility {
        Visibility::Public => score += PUBLIC_BONUS,
        Visibility::Crate => score += CRATE_BONUS,
        Visibility::Protected => {}
        Visibility::Private if !same_file => score += PRIVATE_CROSS_FILE_PENALTY,
        Visibility::Private | Visibility::Unknown => {}
    }

    let kind_matches = if call_ref.is_method {
        candidate.kind == SymbolKind::Method
    } else {
        matches!(candidate.kind, SymbolKind::Function | SymbolKind::Constant)
    };
    if kind_matches {
        score += KIND_MATCH_BONUS;
    }

    if let Some(prefix) = &call_ref.callee_prefix {
        if candidate
            .qualified_name
            .split("::")
            .any(|component| component == prefix)
        {
            score += PREFIX_MATCH_BONUS;
        }
    }

    score
}

fn build_calls_edge(
    from_file: FileNodeId,
    callee: SymbolNodeId,
    revision: &str,
    file_path: &str,
) -> Edge {
    Edge {
        id: derive_edge_id(
            NodeId::File(from_file),
            NodeId::Symbol(callee),
            EdgeKind::Calls,
        ),
        from: NodeId::File(from_file),
        to: NodeId::Symbol(callee),
        kind: EdgeKind::Calls,
        owner_file_id: None,
        last_observed_rev: None,
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        drift_score: 0.0,
        provenance: make_provenance("stage4_calls", revision, file_path, ""),
    }
}
