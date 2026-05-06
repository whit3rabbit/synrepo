use serde_json::{json, Value};

use crate::core::ids::FileNodeId;
use crate::structure::graph::{Edge, EdgeKind, GraphReader};
use crate::surface::card::Budget;

pub(super) fn file_outline_content(
    graph: &dyn GraphReader,
    file_id: FileNodeId,
    budget: Budget,
) -> crate::Result<Value> {
    let file = graph
        .get_file(file_id)?
        .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("file {file_id} not found")))?;
    let symbol_limit = match budget {
        Budget::Tiny => 10,
        Budget::Normal => 40,
        Budget::Deep => usize::MAX,
    };
    let symbols: Vec<Value> = graph
        .symbols_for_file(file_id)?
        .into_iter()
        .take(symbol_limit)
        .map(|sym| {
            json!({
                "id": sym.id,
                "qualified_name": sym.qualified_name,
                "kind": sym.kind,
                "visibility": sym.visibility,
                "signature": sym.signature,
                "doc_comment": if budget == Budget::Tiny { None } else { sym.doc_comment },
                "body_hash": sym.body_hash,
                "body_byte_range": sym.body_byte_range,
            })
        })
        .collect();

    let (imports, imported_by) = if budget == Budget::Tiny {
        (Vec::new(), Vec::new())
    } else {
        (
            file_refs(
                graph,
                graph.outbound(crate::NodeId::File(file_id), Some(EdgeKind::Imports))?,
                true,
            )?,
            file_refs(
                graph,
                graph.inbound(crate::NodeId::File(file_id), Some(EdgeKind::Imports))?,
                false,
            )?,
        )
    };

    let content_hash = file.content_hash;
    let mut content = json!({
        "path": file.path,
        "file": file.id,
        "language": file.language,
        "size_bytes": file.size_bytes,
        "content_hash": content_hash,
        "symbol_count": symbols.len(),
        "symbols": symbols,
        "imports": imports,
        "imported_by": imported_by,
    });
    super::artifacts::attach_estimated_accounting(&mut content, budget);
    if let Some(accounting) = content
        .get_mut("context_accounting")
        .and_then(|value| value.as_object_mut())
    {
        accounting.insert("source_hashes".to_string(), json!([content_hash]));
    }
    Ok(content)
}

fn file_refs(
    graph: &dyn GraphReader,
    edges: Vec<Edge>,
    outbound: bool,
) -> crate::Result<Vec<Value>> {
    let mut refs = Vec::new();
    for edge in edges {
        let node = if outbound { edge.to } else { edge.from };
        if let crate::NodeId::File(id) = node {
            if let Some(file) = graph.get_file(id)? {
                refs.push(json!({ "id": id, "path": file.path }));
            }
        }
    }
    Ok(refs)
}
