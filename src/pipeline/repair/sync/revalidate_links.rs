//! `RevalidateLinks` repair action: re-run the fuzzy-LCS verifier against
//! stale cross-link candidates and update their stored hashes and verified
//! spans in place when the cited text still matches current source/target.
//!
//! Extracted from `handlers.rs` to keep that file under the 400-line cap.

use std::str::FromStr;

use crate::{
    core::ids::NodeId,
    overlay::{OverlayEdgeKind, OverlayStore},
    pipeline::repair::{
        cross_link_verify::verify_candidate_payload, DriftClass, RepairAction, RepairFinding,
        Severity,
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
};

use super::handlers::ActionContext;

/// Revalidate a stale cross-link candidate.
///
/// * Verifier `Ok(Some)` — spans re-located; stored hashes and spans are
///   updated in place and the finding moves to `repaired`.
/// * Verifier `Ok(None)` — spans no longer locatable; finding stays in
///   `report_only`. Do not auto-reject; that is a manual call.
/// * Verifier `Err` — push to `blocked` mirroring the `RegenerateExports`
///   blocked path.
pub(super) fn handle_revalidate_links(
    finding: &RepairFinding,
    context: &ActionContext<'_>,
    repaired: &mut Vec<RepairFinding>,
    report_only: &mut Vec<RepairFinding>,
    blocked: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) {
    let Some(target_id) = finding.target_id.as_deref() else {
        actions_taken.push("cross-link revalidation skipped: no target_id".to_string());
        report_only.push(finding.clone());
        return;
    };

    let Some(RevalidateTarget { from, to, kind }) = parse_revalidate_target(target_id) else {
        actions_taken.push(format!(
            "cross-link revalidation skipped: malformed target_id `{target_id}`"
        ));
        report_only.push(finding.clone());
        return;
    };

    let graph = match SqliteGraphStore::open_existing(&context.synrepo_dir.join("graph")) {
        Ok(g) => g,
        Err(err) => {
            actions_taken.push(format!(
                "cross-link revalidation could not open graph: {err}"
            ));
            report_only.push(finding.clone());
            return;
        }
    };
    let mut overlay = match SqliteOverlayStore::open_existing(&context.synrepo_dir.join("overlay"))
    {
        Ok(o) => o,
        Err(err) => {
            actions_taken.push(format!(
                "cross-link revalidation could not open overlay: {err}"
            ));
            report_only.push(finding.clone());
            return;
        }
    };

    let candidate = match overlay.candidate_by_endpoints(from, to, kind) {
        Ok(Some(c)) => c,
        Ok(None) => {
            actions_taken.push(format!(
                "cross-link revalidation skipped: candidate no longer present for {target_id}"
            ));
            report_only.push(finding.clone());
            return;
        }
        Err(err) => {
            actions_taken.push(format!(
                "cross-link revalidation failed to read candidate: {err}"
            ));
            blocked.push(blocked_finding(
                finding,
                format!("Candidate lookup failed: {err}"),
            ));
            return;
        }
    };

    match verify_candidate_payload(&graph, context.repo_root, &candidate) {
        Ok(Some(verified)) => match overlay.revalidate_link(
            from,
            to,
            kind,
            &verified.from_hash,
            &verified.to_hash,
            &verified.source_spans,
            &verified.target_spans,
        ) {
            Ok(()) => {
                actions_taken.push(format!(
                    "revalidated cross-link {from}->{to}/{}",
                    kind.as_str()
                ));
                repaired.push(finding.clone());
            }
            Err(err) => {
                actions_taken.push(format!(
                    "cross-link revalidation write failed for {target_id}: {err}"
                ));
                blocked.push(blocked_finding(
                    finding,
                    format!("Revalidation write failed: {err}"),
                ));
            }
        },
        Ok(None) => {
            actions_taken.push(format!(
                "cross-link revalidation reports no match for {target_id}: cited spans not found in current source/target"
            ));
            let mut r = finding.clone();
            r.notes = Some(
                "Verifier could not re-locate cited spans in current source/target text."
                    .to_string(),
            );
            report_only.push(r);
        }
        Err(err) => {
            actions_taken.push(format!(
                "cross-link revalidation errored for {target_id}: {err}"
            ));
            blocked.push(blocked_finding(
                finding,
                format!("Revalidation verifier failed: {err}"),
            ));
        }
    }
}

/// Parsed form of the `RepairFinding.target_id` string set by
/// `ProposedLinksOverlayCheck::drifted_cross_link_finding`. The stored
/// format is `"from={NodeId} to={NodeId} kind={label}"`.
struct RevalidateTarget {
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
}

fn parse_revalidate_target(target: &str) -> Option<RevalidateTarget> {
    let mut from: Option<NodeId> = None;
    let mut to: Option<NodeId> = None;
    let mut kind: Option<OverlayEdgeKind> = None;
    for chunk in target.split_whitespace() {
        let (key, value) = chunk.split_once('=')?;
        match key {
            "from" => from = NodeId::from_str(value).ok(),
            "to" => to = NodeId::from_str(value).ok(),
            "kind" => kind = OverlayEdgeKind::from_str_label(value),
            _ => return None,
        }
    }
    Some(RevalidateTarget {
        from: from?,
        to: to?,
        kind: kind?,
    })
}

fn blocked_finding(finding: &RepairFinding, notes: String) -> RepairFinding {
    let mut b = finding.clone();
    b.drift_class = DriftClass::Blocked;
    b.severity = Severity::Blocked;
    b.recommended_action = RepairAction::ManualReview;
    b.notes = Some(notes);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::core::ids::{FileNodeId, NodeId};

    #[test]
    fn parses_well_formed_target_id() {
        let from = NodeId::File(FileNodeId(1));
        let to = NodeId::File(FileNodeId(2));
        let target = format!("from={from} to={to} kind=references");
        let parsed = parse_revalidate_target(&target).expect("should parse");
        assert_eq!(parsed.kind, OverlayEdgeKind::References);
        assert_eq!(parsed.from.to_string(), from.to_string());
        assert_eq!(parsed.to.to_string(), to.to_string());
    }

    #[test]
    fn rejects_malformed_target_id() {
        assert!(parse_revalidate_target("not a target").is_none());
        assert!(parse_revalidate_target("from=bogus to=bogus kind=bogus").is_none());
        let from = NodeId::File(FileNodeId(1));
        assert!(parse_revalidate_target(&format!("from={from}")).is_none());
    }
}
