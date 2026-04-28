use crate::{
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    store::sqlite::SqliteGraphStore,
    structure::graph::EdgeKind,
};

pub(super) fn symbol_named(graph: &SqliteGraphStore, file: FileNodeId, name: &str) -> SymbolNodeId {
    graph
        .outbound(NodeId::File(file), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .find_map(|edge| match edge.to {
            NodeId::Symbol(id) => {
                let symbol = graph.get_symbol(id).ok()??;
                (symbol.display_name == name).then_some(id)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("symbol {name} must exist"))
}

pub(super) fn assert_symbol_call(
    graph: &SqliteGraphStore,
    caller: SymbolNodeId,
    callee: SymbolNodeId,
) {
    let calls = graph
        .outbound(NodeId::Symbol(caller), Some(EdgeKind::Calls))
        .unwrap();
    assert!(
        calls.iter().any(|edge| edge.to == NodeId::Symbol(callee)),
        "expected symbol Calls edge from {caller} to {callee}; got: {calls:?}"
    );
}
