//! Drift scoring for graph edges (stage 7).
//!
//! Each edge carries a drift score in `[0.0, 1.0]`, where 0 means both
//! endpoints' structural fingerprints are unchanged since the prior compile
//! cycle and 1 means an endpoint was deleted or the fingerprints are entirely
//! disjoint.
//!
//! Scoring uses Jaccard distance on per-file structural fingerprints:
//! the sorted set of `(qualified_name, signature_hash)` pairs for all symbols
//! in each file. At each compile cycle, the prior revision's fingerprints are
//! read from the `file_fingerprints` sidecar table, current fingerprints are
//! computed from the graph, and the distance between them determines drift.
//!
//! Scoring bands:
//! - 0.0: identical fingerprints (no structural change)
//! - 0.0-0.3: minor divergence (added/removed symbols)
//! - 0.3-0.7: moderate divergence (signature changes, significant turnover)
//! - 0.7-1.0: high divergence (major structural change)
//! - 1.0: endpoint deleted
//!
//! Concept edges are scored against the non-concept endpoint's file fingerprint.
//! Concept-to-concept edges score 0.0 (no structural fingerprint to compare).
//! Defines edges within the same file score 0.0 (the symbol is part of the
//! file's own fingerprint).

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::core::ids::{FileNodeId, NodeId};
use crate::structure::graph::{Edge, EdgeKind, GraphStore, SymbolNode};

/// A structural fingerprint: the sorted set of (qualified_name, signature_hash)
/// pairs for all symbols in a file. Used to measure how much a file's API surface
/// has changed between compile cycles.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct StructuralFingerprint {
    pairs: BTreeSet<FingerprintEntry>,
}

/// A single (qualified_name, signature_hash) entry in a fingerprint.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
struct FingerprintEntry {
    qualified_name: String,
    /// Hash of the symbol's signature string (not the body).
    signature_hash: u64,
}

impl StructuralFingerprint {
    /// Build a fingerprint from an iterator of (qualified_name, signature_hash) pairs.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (String, u64)>) -> Self {
        Self {
            pairs: pairs
                .into_iter()
                .map(|(name, hash)| FingerprintEntry {
                    qualified_name: name,
                    signature_hash: hash,
                })
                .collect(),
        }
    }

    /// Compute Jaccard distance (1 - Jaccard similarity) between two fingerprints.
    /// Returns 1.0 when one or both fingerprints are empty and not identical.
    pub fn jaccard_distance(&self, other: &Self) -> f32 {
        if self.pairs.is_empty() && other.pairs.is_empty() {
            return 0.0;
        }
        if self.pairs.is_empty() || other.pairs.is_empty() {
            return 1.0;
        }

        let intersection = self.pairs.intersection(&other.pairs).count() as f64;
        let union = self.pairs.union(&other.pairs).count() as f64;
        1.0 - (intersection / union) as f32
    }

    /// Number of entries in the fingerprint.
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Whether the fingerprint is empty.
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

/// Collect the structural fingerprint for a file: all its symbols'
/// (qualified_name, signature_hash) pairs.
pub fn fingerprint_for_file(
    file_id: FileNodeId,
    graph: &dyn GraphStore,
) -> crate::Result<StructuralFingerprint> {
    let edges = graph.outbound(NodeId::File(file_id), Some(EdgeKind::Defines))?;
    let mut pairs = Vec::new();
    for edge in &edges {
        if let NodeId::Symbol(sym_id) = edge.to {
            if let Some(sym) = graph.get_symbol(sym_id)? {
                let hash = signature_hash(&sym);
                pairs.push((sym.qualified_name.clone(), hash));
            }
        }
    }
    Ok(StructuralFingerprint::from_pairs(pairs))
}

/// Hash a symbol's signature. Returns 0 when no signature is present,
/// which treats unsignatured symbols as identical (they have no declared
/// API surface to compare).
fn signature_hash(sym: &SymbolNode) -> u64 {
    match &sym.signature {
        Some(sig) => {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            sig.hash(&mut hasher);
            hasher.finish()
        }
        None => 0,
    }
}

/// Compute a drift score for an edge given prior and current fingerprints.
///
/// Each endpoint is resolved to its file-level fingerprint. The score is the
/// average of both endpoints' Jaccard distances between prior and current.
/// Deleted endpoints (present in prior but absent from current) score 1.0.
/// Concept endpoints contribute drift from the non-concept side only.
pub fn compute_drift_score(
    edge: &Edge,
    prior: &HashMap<FileNodeId, StructuralFingerprint>,
    current: &HashMap<FileNodeId, StructuralFingerprint>,
    graph: &dyn GraphStore,
) -> crate::Result<f32> {
    let from_file_id = match edge.from {
        NodeId::File(id) => Some(id),
        NodeId::Symbol(sym_id) => graph.get_symbol(sym_id)?.map(|s| s.file_id),
        NodeId::Concept(_) => None,
    };

    let to_file_id = match edge.to {
        NodeId::File(id) => Some(id),
        NodeId::Symbol(sym_id) => graph.get_symbol(sym_id)?.map(|s| s.file_id),
        NodeId::Concept(_) => None,
    };

    // Defines edges within the same file: the symbol is part of the file's
    // own fingerprint, so drift is captured by the file-level comparison.
    if matches!(edge.kind, EdgeKind::Defines) {
        if let (Some(from_fid), Some(to_fid)) = (from_file_id, to_file_id) {
            if from_fid == to_fid {
                return Ok(0.0);
            }
        }
    }

    // Concept-to-concept edges: no structural fingerprint to compare.
    if from_file_id.is_none() && to_file_id.is_none() {
        return Ok(0.0);
    }

    // For concept-involved edges, only score the non-concept endpoint.
    let (Some(from_fid), Some(to_fid)) = (from_file_id, to_file_id) else {
        // Exactly one side is a concept. Score the non-concept side only.
        let non_concept_id = from_file_id.or(to_file_id).unwrap();
        return Ok(endpoint_drift(non_concept_id, prior, current));
    };

    // Both endpoints have file IDs: average their drifts.
    let from_drift = endpoint_drift(from_fid, prior, current);
    let to_drift = endpoint_drift(to_fid, prior, current);
    Ok((from_drift + to_drift) / 2.0)
}

/// Compute drift for a single endpoint file between prior and current
/// fingerprint snapshots. Returns 1.0 if the file was deleted (present in
/// prior but absent from current) or if this is the first cycle (no prior).
fn endpoint_drift(
    file_id: FileNodeId,
    prior: &HashMap<FileNodeId, StructuralFingerprint>,
    current: &HashMap<FileNodeId, StructuralFingerprint>,
) -> f32 {
    let prior_fp = match prior.get(&file_id) {
        Some(fp) => fp,
        None => {
            // No prior fingerprint: either the file is new this cycle or this
            // is the first compile. New files don't have drift against a
            // prior state, so return 0.0.
            return 0.0;
        }
    };
    let current_fp = match current.get(&file_id) {
        Some(fp) => fp,
        None => {
            // File existed in prior but is gone now: deleted endpoint.
            return 1.0;
        }
    };
    prior_fp.jaccard_distance(current_fp)
}

/// Run stage 7: compute drift scores for all edges and persist them.
///
/// Reads the prior revision's fingerprints from the sidecar table, computes
/// current fingerprints from the graph, then writes drift scores and the
/// current fingerprints for the next cycle. Old revisions are truncated.
pub fn run_drift_scoring(graph: &mut dyn GraphStore, revision: &str) -> crate::Result<usize> {
    // Read prior fingerprints (empty on first run).
    let prior_revision = graph.latest_fingerprint_revision()?;
    let prior = match &prior_revision {
        Some(rev) => graph.read_fingerprints(rev)?,
        None => HashMap::new(),
    };

    // Compute current fingerprints for every file.
    let file_paths = graph.all_file_paths()?;
    let mut current = HashMap::new();
    for (_, file_id) in &file_paths {
        current.insert(*file_id, fingerprint_for_file(*file_id, graph)?);
    }

    // Enumerate all edges (not just file-outbound).
    let all_edges = graph.all_edges()?;

    // Compute drift scores.
    let mut scores = Vec::new();
    for edge in &all_edges {
        let score = compute_drift_score(edge, &prior, &current, graph)?;
        if score > 0.0 {
            scores.push((edge.id, score));
        }
    }

    let scored_count = scores.len();

    // Truncate old drift scores and fingerprints before writing new ones.
    graph.truncate_drift_scores(revision)?;
    graph.truncate_fingerprints(revision)?;

    // Write current revision's drift scores.
    if !scores.is_empty() {
        graph.write_drift_scores(&scores, revision)?;
    }

    // Write current fingerprints for the next cycle to compare against.
    let fp_entries: Vec<_> = current.into_iter().collect();
    if !fp_entries.is_empty() {
        graph.write_fingerprints(&fp_entries, revision)?;
    }

    Ok(scored_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_fingerprints_have_zero_distance() {
        let fp = StructuralFingerprint::from_pairs([
            ("foo::bar".to_string(), 42),
            ("foo::baz".to_string(), 99),
        ]);
        assert!((fp.jaccard_distance(&fp) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn disjoint_fingerprints_have_distance_one() {
        let fp1 = StructuralFingerprint::from_pairs([("a".to_string(), 1)]);
        let fp2 = StructuralFingerprint::from_pairs([("b".to_string(), 2)]);
        assert!((fp1.jaccard_distance(&fp2) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn partial_overlap_gives_intermediate_distance() {
        let fp1 = StructuralFingerprint::from_pairs([
            ("a".to_string(), 1),
            ("b".to_string(), 2),
            ("c".to_string(), 3),
        ]);
        let fp2 = StructuralFingerprint::from_pairs([
            ("a".to_string(), 1),
            ("b".to_string(), 2),
            ("d".to_string(), 4),
        ]);
        // Intersection = {a, b} = 2, Union = {a, b, c, d} = 4
        // Distance = 1 - 2/4 = 0.5
        assert!((fp1.jaccard_distance(&fp2) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn added_symbols_reduce_similarity() {
        let fp1 = StructuralFingerprint::from_pairs([("a".to_string(), 1)]);
        let fp2 = StructuralFingerprint::from_pairs([("a".to_string(), 1), ("b".to_string(), 2)]);
        // Intersection = {a} = 1, Union = {a, b} = 2
        // Distance = 1 - 1/2 = 0.5
        assert!((fp1.jaccard_distance(&fp2) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn removed_symbols_reduce_similarity() {
        let fp1 = StructuralFingerprint::from_pairs([("a".to_string(), 1), ("b".to_string(), 2)]);
        let fp2 = StructuralFingerprint::from_pairs([("a".to_string(), 1)]);
        assert!((fp1.jaccard_distance(&fp2) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn changed_signature_counts_as_different() {
        let fp1 = StructuralFingerprint::from_pairs([("a".to_string(), 1)]);
        let fp2 = StructuralFingerprint::from_pairs([("a".to_string(), 2)]);
        // Same name, different hash -> disjoint at the entry level.
        assert!((fp1.jaccard_distance(&fp2) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn both_empty_fingerprints_have_zero_distance() {
        let fp1 = StructuralFingerprint::default();
        let fp2 = StructuralFingerprint::default();
        assert!((fp1.jaccard_distance(&fp2) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn one_empty_fingerprint_has_distance_one() {
        let fp1 = StructuralFingerprint::default();
        let fp2 = StructuralFingerprint::from_pairs([("a".to_string(), 1)]);
        assert!((fp1.jaccard_distance(&fp2) - 1.0).abs() < f32::EPSILON);
    }
}
