use crate::{
    core::ids::NodeId,
    overlay::{derive_link_freshness, ConfidenceTier, OverlayStore},
    structure::graph::GraphStore,
    surface::card::types::ProposedLink,
};

/// Resolve proposed links for a given node at Deep budget.
///
/// Filters out links classified as `BelowThreshold`. Computes the current
/// freshness of each link against the canonical graph's content hashes.
pub(super) fn resolve_proposed_links(
    overlay: Option<&parking_lot::Mutex<dyn OverlayStore>>,
    graph: &dyn GraphStore,
    node: NodeId,
) -> crate::Result<(Option<Vec<ProposedLink>>, &'static str)> {
    let overlay = match overlay {
        Some(o) => o,
        None => return Ok((None, "missing")),
    };

    let candidates = overlay.lock().links_for(node)?;
    if candidates.is_empty() {
        return Ok((None, "missing"));
    }

    let mut proposed = Vec::new();

    for link in candidates {
        // Task 5.4: Filter out below_threshold candidates
        if link.confidence_tier == ConfidenceTier::BelowThreshold {
            continue;
        }

        // To determine freshness, we need the current content hashes of the endpoints from the graph.
        let from_hash = get_node_content_hash(graph, link.from)?;
        let to_hash = get_node_content_hash(graph, link.to)?;

        let freshness = derive_link_freshness(&link, from_hash.as_deref(), to_hash.as_deref());

        proposed.push(ProposedLink {
            source: link.from,
            target: link.to,
            kind: link.kind,
            tier: link.confidence_tier,
            freshness,
            span_count: link.source_spans.len() + link.target_spans.len(),
        });
    }

    let state = if proposed.is_empty() {
        "missing"
    } else {
        // Technically wait, is the overall state "missing" if all are below threshold?
        // Let's say "present" because we don't have a single overall freshness like commentary.
        // We'll just say "present" or "missing" or something. But wait, what does the spec say?
        // Actually, the spec just says `links_state` is `"budget_withheld"` at Tiny/Normal.
        // And maybe `"present"` or `"missing"` at Deep. Let's just use "present" if proposed.len() > 0.
        "present"
    };

    Ok((Some(proposed), state))
}

fn get_node_content_hash(graph: &dyn GraphStore, node: NodeId) -> crate::Result<Option<String>> {
    match node {
        NodeId::File(id) => Ok(graph.get_file(id)?.map(|f| f.content_hash)),
        NodeId::Symbol(id) => {
            if let Some(sym) = graph.get_symbol(id)? {
                Ok(graph.get_file(sym.file_id)?.map(|f| f.content_hash))
            } else {
                Ok(None)
            }
        }
        NodeId::Concept(id) => {
            if let Some(c) = graph.get_concept(id)? {
                Ok(graph.file_by_path(&c.path)?.map(|f| f.content_hash))
            } else {
                Ok(None)
            }
        }
    }
}
