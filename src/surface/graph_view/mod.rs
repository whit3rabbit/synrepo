//! Shared bounded graph-neighborhood model for CLI, TUI, and MCP views.

mod builder;
#[cfg(test)]
mod tests;
mod types;

pub use builder::{
    build_graph_neighborhood, build_graph_neighborhood_with_compiler, parse_edge_kind_filter,
    parse_edge_kind_filters,
};
pub use types::{
    GraphNeighborhood, GraphNeighborhoodRequest, GraphViewCounts, GraphViewDegree,
    GraphViewDirection, GraphViewEdge, GraphViewNode,
};
