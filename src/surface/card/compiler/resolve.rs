use crate::{core::ids::NodeId, structure::graph::GraphStore};

pub(super) fn resolve_target(
    graph: &dyn GraphStore,
    target: &str,
) -> crate::Result<Option<NodeId>> {
    // 1. Try exact file path lookup.
    if let Some(file) = graph.file_by_path(target)? {
        return Ok(Some(NodeId::File(file.id)));
    }

    // 2. Try symbol name match (display name or qualified name suffix).
    let all_syms = graph.all_symbol_names()?;
    for (sym_id, _file_id, qname) in &all_syms {
        let short = qname.rsplit("::").next().unwrap_or(qname.as_str());
        if qname == target || short == target {
            return Ok(Some(NodeId::Symbol(*sym_id)));
        }
    }

    // 3. Try substring match on qualified name.
    for (sym_id, _file_id, qname) in &all_syms {
        if qname.contains(target) {
            return Ok(Some(NodeId::Symbol(*sym_id)));
        }
    }

    Ok(None)
}
