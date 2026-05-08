use crate::{
    core::ids::NodeId,
    structure::graph::{EdgeKind, GraphReader, GraphStore},
    surface::card::{Budget, DecisionCard, Freshness},
};

use super::SynrepoState;

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

/// Execute an MCP tool handler with a fresh, request-local compiler.
///
/// Encapsulates the per-request store connection and snapshot isolation.
/// The result is rendered to a JSON string via [`render_result`].
pub fn with_mcp_compiler<R>(
    state: &SynrepoState,
    f: impl FnOnce(&crate::surface::card::compiler::GraphCardCompiler) -> anyhow::Result<R>,
) -> String
where
    R: serde::Serialize,
{
    let result = state
        .with_read_compiler(|compiler| {
            let val = f(compiler).map_err(crate::Error::from)?;
            serde_json::to_value(val).map_err(|err| crate::Error::Other(anyhow::anyhow!(err)))
        })
        .map_err(|e| anyhow::anyhow!(e));
    render_result(result)
}

pub fn parse_budget(s: &str) -> anyhow::Result<Budget> {
    match s.trim().to_ascii_lowercase().as_str() {
        "tiny" => Ok(Budget::Tiny),
        "normal" => Ok(Budget::Normal),
        "deep" => Ok(Budget::Deep),
        _ => Err(super::error::McpError::invalid_parameter(format!(
            "invalid budget: {s}; expected tiny, normal, or deep"
        ))
        .into()),
    }
}

pub fn render_result(result: anyhow::Result<serde_json::Value>) -> String {
    match result {
        Ok(val) => super::response_budget::serialize_mcp_json(&val)
            .unwrap_or_else(|e| super::error::error_json(anyhow::anyhow!(e))),
        Err(err) => super::error::error_json(err),
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
    graph: &dyn GraphReader,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_parser_rejects_unknown_tiers() {
        assert_eq!(parse_budget("tiny").unwrap(), Budget::Tiny);
        assert_eq!(parse_budget("normal").unwrap(), Budget::Normal);
        assert_eq!(parse_budget("deep").unwrap(), Budget::Deep);

        let err = parse_budget("deeep").unwrap_err();
        let rendered = super::render_result(Err(err));
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "INVALID_PARAMETER");
        assert!(value["error_message"]
            .as_str()
            .unwrap()
            .contains("invalid budget"));
    }
}
