use crate::{
    core::ids::NodeId,
    structure::graph::{EdgeKind, GraphStore},
    surface::card::{Budget, DecisionCard, Freshness},
};

/// Hold a read snapshot across the whole handler body.
///
/// The graph snapshot methods are re-entrant, so wrapping here composes
/// safely with the per-call wraps inside `GraphCardCompiler`. Any error
/// from `end_read_snapshot` is intentionally swallowed (debug-logged) so
/// the handler's original `Err` is never masked.
pub fn with_graph_snapshot<R>(
    graph: &dyn GraphStore,
    f: impl FnOnce() -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    graph.begin_read_snapshot()?;
    let out = f();
    let _ = graph.end_read_snapshot();
    out
}

pub fn parse_budget(s: &str) -> Budget {
    match s.to_ascii_lowercase().as_str() {
        "normal" => Budget::Normal,
        "deep" => Budget::Deep,
        _ => Budget::Tiny,
    }
}

pub fn render_result(result: anyhow::Result<serde_json::Value>) -> String {
    match result {
        Ok(val) => serde_json::to_string_pretty(&val)
            .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string()),
        Err(err) => serde_json::to_string_pretty(&serde_json::json!({ "error": err.to_string() }))
            .unwrap_or_else(|_| r#"{"error":"serialization failure"}"#.to_string()),
    }
}

/// Mirror `overlay_commentary.text` onto a top-level `commentary_text` key
/// so MCP callers can branch on a flat field without traversing the nested
/// object. Absent when there is no commentary to surface.
pub fn lift_commentary_text(json: &mut serde_json::Value) {
    let Some(obj) = json.as_object_mut() else {
        return;
    };
    let text = obj
        .get("overlay_commentary")
        .and_then(|oc| oc.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    if let Some(t) = text {
        obj.insert("commentary_text".to_string(), serde_json::Value::String(t));
    }
}

/// If `node_id` has incoming Governs edges, build DecisionCards and attach
/// them to the JSON card object under the key `"decision_cards"`.
/// The key is absent (not null) when no governing concepts exist.
pub fn attach_decision_cards(
    json: &mut serde_json::Value,
    node_id: NodeId,
    graph: &dyn crate::structure::graph::GraphStore,
    budget: Budget,
) -> anyhow::Result<()> {
    let concepts = graph.find_governing_concepts(node_id)?;
    if concepts.is_empty() {
        return Ok(());
    }

    let mut cards: Vec<serde_json::Value> = Vec::new();
    for concept in concepts {
        let governs_edges = graph.outbound(NodeId::Concept(concept.id), Some(EdgeKind::Governs))?;
        let governed_node_ids: Vec<NodeId> = governs_edges.iter().map(|e| e.to).collect();

        let dc = DecisionCard {
            title: concept.title.clone(),
            status: concept.status.clone(),
            decision_body: concept.decision_body.clone(),
            governed_node_ids,
            source_path: concept.path.clone(),
            freshness: Freshness::Fresh,
        };
        cards.push(dc.render(budget));
    }

    if let serde_json::Value::Object(ref mut map) = json {
        map.insert(
            "decision_cards".to_string(),
            serde_json::Value::Array(cards),
        );
    }
    Ok(())
}
