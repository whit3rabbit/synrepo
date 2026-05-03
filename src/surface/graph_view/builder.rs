use std::collections::{HashMap, HashSet, VecDeque};

use crate::{
    core::ids::{EdgeId, NodeId},
    structure::graph::{Edge, EdgeKind, GraphReader},
    surface::{
        card::compiler::{resolve_target, GraphCardCompiler},
        graph_view::types::{GraphViewDegree, GraphViewEdge},
    },
};

use super::types::{
    GraphNeighborhood, GraphNeighborhoodRequest, GraphViewCounts, GraphViewDirection, GraphViewNode,
};

const DEFAULT_DEPTH: usize = 1;
const DEFAULT_LIMIT: usize = 100;
const MAX_DEPTH: usize = 3;
const MAX_LIMIT: usize = 500;

type NeighborhoodParts = (Option<NodeId>, Vec<NodeId>, Vec<Edge>, bool);

/// Build a bounded graph-neighborhood model against a reader.
///
/// Callers that hold a SQLite store should wrap this in `with_graph_read_snapshot`.
pub fn build_graph_neighborhood(
    graph: &dyn GraphReader,
    request: GraphNeighborhoodRequest,
) -> crate::Result<GraphNeighborhood> {
    let request = normalize_request(request);
    let all_edges = filtered_all_edges(graph, &request.edge_types)?;
    let drift_scores = current_drift_scores(graph)?;
    let degree_by_node = degree_map(&all_edges);

    let (focal, nodes, edges, mut truncated) = match request.target.as_deref() {
        Some(target) => build_target_neighborhood(graph, &request, target, &all_edges)?,
        None => build_top_degree_overview(&request, &all_edges, &degree_by_node),
    };

    let mut rendered_nodes = Vec::with_capacity(nodes.len());
    for node_id in nodes {
        if let Some(node) = render_node(graph, node_id, &degree_by_node)? {
            rendered_nodes.push(node);
        } else {
            truncated = true;
        }
    }

    let node_ids = rendered_nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<HashSet<_>>();
    let mut rendered_edges = Vec::new();
    for edge in edges {
        if rendered_edges.len() >= request.limit {
            truncated = true;
            break;
        }
        if node_ids.contains(&edge.from.to_string()) && node_ids.contains(&edge.to.to_string()) {
            rendered_edges.push(render_edge(edge, &drift_scores));
        }
    }

    rendered_nodes.sort_by(|a, b| {
        b.degree
            .total
            .cmp(&a.degree.total)
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.id.cmp(&b.id))
    });
    rendered_edges.sort_by(|a, b| a.id.cmp(&b.id));

    let counts = counts_for(&rendered_nodes, &rendered_edges);
    Ok(GraphNeighborhood {
        target: request.target,
        focal_node_id: focal.map(|node_id| node_id.to_string()),
        direction: request.direction.as_str(),
        depth: request.depth,
        limit: request.limit,
        edge_types: request
            .edge_types
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        counts,
        truncated,
        nodes: rendered_nodes,
        edges: rendered_edges,
        source_store: "graph",
    })
}

/// Build a bounded graph-neighborhood model through a card compiler.
pub fn build_graph_neighborhood_with_compiler(
    compiler: &GraphCardCompiler,
    request: GraphNeighborhoodRequest,
) -> crate::Result<GraphNeighborhood> {
    compiler.with_reader(|graph| build_graph_neighborhood(graph, request))
}

fn normalize_request(mut request: GraphNeighborhoodRequest) -> GraphNeighborhoodRequest {
    if request.depth == 0 {
        request.depth = DEFAULT_DEPTH;
    }
    if request.limit == 0 {
        request.limit = DEFAULT_LIMIT;
    }
    request.depth = request.depth.min(MAX_DEPTH);
    request.limit = request.limit.min(MAX_LIMIT);
    request
}

fn build_target_neighborhood(
    graph: &dyn GraphReader,
    request: &GraphNeighborhoodRequest,
    target: &str,
    all_edges: &[Edge],
) -> crate::Result<NeighborhoodParts> {
    let focal = resolve_target(graph, target)?
        .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("target not found: {target}")))?;

    let edge_by_id = all_edges
        .iter()
        .map(|edge| (edge.id, edge.clone()))
        .collect::<HashMap<_, _>>();
    let mut node_order = Vec::new();
    let mut seen_nodes = HashSet::new();
    let mut edge_ids = Vec::new();
    let mut seen_edges = HashSet::new();
    let mut queue = VecDeque::from([(focal, 0usize)]);
    let mut truncated = false;

    seen_nodes.insert(focal);
    node_order.push(focal);

    while let Some((node, depth)) = queue.pop_front() {
        if depth >= request.depth {
            continue;
        }
        for edge in incident_edges(graph, node, request)? {
            let peer = peer_for(edge.from, edge.to, node);
            if !seen_nodes.contains(&peer) {
                if node_order.len() >= request.limit {
                    truncated = true;
                    continue;
                }
                seen_nodes.insert(peer);
                node_order.push(peer);
                queue.push_back((peer, depth + 1));
            }
            if !seen_edges.contains(&edge.id) {
                if edge_ids.len() >= request.limit {
                    truncated = true;
                    continue;
                }
                seen_edges.insert(edge.id);
                edge_ids.push(edge.id);
            }
        }
    }

    let edges = edge_ids
        .into_iter()
        .filter_map(|edge_id| edge_by_id.get(&edge_id).cloned())
        .collect();
    Ok((Some(focal), node_order, edges, truncated))
}

fn build_top_degree_overview(
    request: &GraphNeighborhoodRequest,
    all_edges: &[Edge],
    degree_by_node: &HashMap<NodeId, GraphViewDegree>,
) -> NeighborhoodParts {
    let mut ranked = degree_by_node.keys().copied().collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        degree_by_node
            .get(b)
            .map(|degree| degree.total)
            .unwrap_or_default()
            .cmp(
                &degree_by_node
                    .get(a)
                    .map(|degree| degree.total)
                    .unwrap_or_default(),
            )
            .then_with(|| a.to_string().cmp(&b.to_string()))
    });

    let truncated = ranked.len() > request.limit;
    ranked.truncate(request.limit);
    let node_set = ranked.iter().copied().collect::<HashSet<_>>();
    let mut edges = all_edges
        .iter()
        .filter(|edge| node_set.contains(&edge.from) && node_set.contains(&edge.to))
        .take(request.limit)
        .cloned()
        .collect::<Vec<_>>();
    edges.sort_by_key(|edge| edge.id);

    let edge_truncated = all_edges
        .iter()
        .filter(|edge| node_set.contains(&edge.from) && node_set.contains(&edge.to))
        .count()
        > edges.len();
    (None, ranked, edges, truncated || edge_truncated)
}

fn filtered_all_edges(graph: &dyn GraphReader, kinds: &[EdgeKind]) -> crate::Result<Vec<Edge>> {
    let edges = graph.all_edges()?;
    Ok(edges
        .into_iter()
        .filter(|edge| kinds.is_empty() || kinds.contains(&edge.kind))
        .collect())
}

fn incident_edges(
    graph: &dyn GraphReader,
    node: NodeId,
    request: &GraphNeighborhoodRequest,
) -> crate::Result<Vec<Edge>> {
    let mut edges = Vec::new();
    if matches!(
        request.direction,
        GraphViewDirection::Both | GraphViewDirection::Outbound
    ) {
        edges.extend(directed_edges(graph, node, request, true)?);
    }
    if matches!(
        request.direction,
        GraphViewDirection::Both | GraphViewDirection::Inbound
    ) {
        edges.extend(directed_edges(graph, node, request, false)?);
    }
    edges.sort_by_key(|edge| edge.id);
    edges.dedup_by_key(|edge| edge.id);
    Ok(edges)
}

fn directed_edges(
    graph: &dyn GraphReader,
    node: NodeId,
    request: &GraphNeighborhoodRequest,
    outbound: bool,
) -> crate::Result<Vec<Edge>> {
    if request.edge_types.len() == 1 {
        return if outbound {
            graph.outbound(node, Some(request.edge_types[0]))
        } else {
            graph.inbound(node, Some(request.edge_types[0]))
        };
    }
    let edges = if outbound {
        graph.outbound(node, None)?
    } else {
        graph.inbound(node, None)?
    };
    Ok(edges
        .into_iter()
        .filter(|edge| request.edge_types.is_empty() || request.edge_types.contains(&edge.kind))
        .collect())
}

fn peer_for(from: NodeId, to: NodeId, node: NodeId) -> NodeId {
    if from == node {
        to
    } else {
        from
    }
}

fn degree_map(edges: &[Edge]) -> HashMap<NodeId, GraphViewDegree> {
    let mut out = HashMap::<NodeId, GraphViewDegree>::new();
    for edge in edges {
        let from_degree = out.entry(edge.from).or_default();
        from_degree.outbound += 1;
        from_degree.total += 1;
        let to_degree = out.entry(edge.to).or_default();
        to_degree.inbound += 1;
        to_degree.total += 1;
    }
    out
}

fn render_node(
    graph: &dyn GraphReader,
    node_id: NodeId,
    degree_by_node: &HashMap<NodeId, GraphViewDegree>,
) -> crate::Result<Option<GraphViewNode>> {
    let degree = degree_by_node.get(&node_id).copied().unwrap_or_default();
    Ok(match node_id {
        NodeId::File(file_id) => graph.get_file(file_id)?.map(|file| GraphViewNode {
            id: node_id.to_string(),
            node_type: "file",
            label: file.path.clone(),
            path: Some(file.path),
            file_id: None,
            degree,
        }),
        NodeId::Symbol(symbol_id) => graph.get_symbol(symbol_id)?.map(|symbol| {
            let path = graph
                .get_file(symbol.file_id)
                .ok()
                .flatten()
                .map(|file| file.path);
            GraphViewNode {
                id: node_id.to_string(),
                node_type: "symbol",
                label: symbol.qualified_name,
                path,
                file_id: Some(NodeId::File(symbol.file_id).to_string()),
                degree,
            }
        }),
        NodeId::Concept(concept_id) => {
            graph.get_concept(concept_id)?.map(|concept| GraphViewNode {
                id: node_id.to_string(),
                node_type: "concept",
                label: concept.title,
                path: Some(concept.path),
                file_id: None,
                degree,
            })
        }
    })
}

fn render_edge(edge: Edge, drift_scores: &HashMap<EdgeId, f32>) -> GraphViewEdge {
    GraphViewEdge {
        id: edge.id.to_string(),
        from: edge.from.to_string(),
        to: edge.to.to_string(),
        kind: edge.kind.as_str().to_string(),
        drift_score: drift_scores.get(&edge.id).copied().unwrap_or(0.0),
        epistemic: edge.epistemic,
        provenance: edge.provenance,
    }
}

fn current_drift_scores(graph: &dyn GraphReader) -> crate::Result<HashMap<EdgeId, f32>> {
    let Some(revision) = graph.latest_drift_revision()? else {
        return Ok(HashMap::new());
    };
    Ok(graph.read_drift_scores(&revision)?.into_iter().collect())
}

fn counts_for(nodes: &[GraphViewNode], edges: &[GraphViewEdge]) -> GraphViewCounts {
    let mut counts = GraphViewCounts {
        nodes: nodes.len(),
        edges: edges.len(),
        ..GraphViewCounts::default()
    };
    for node in nodes {
        match node.node_type {
            "file" => counts.files += 1,
            "symbol" => counts.symbols += 1,
            "concept" => counts.concepts += 1,
            _ => {}
        }
    }
    for edge in edges {
        *counts.edges_by_kind.entry(edge.kind.clone()).or_insert(0) += 1;
    }
    counts
}

/// Parse graph edge-kind filters from user input.
pub fn parse_edge_kind_filters(values: &[String]) -> crate::Result<Vec<EdgeKind>> {
    values
        .iter()
        .map(|value| parse_edge_kind_filter(value))
        .collect()
}

/// Parse one edge-kind filter, accepting snake_case and CamelCase labels.
pub fn parse_edge_kind_filter(value: &str) -> crate::Result<EdgeKind> {
    if let Ok(kind) = value.parse::<EdgeKind>() {
        return Ok(kind);
    }
    let mut snake = String::new();
    let mut prev_lower_or_digit = false;
    for ch in value.chars() {
        if ch.is_uppercase() && prev_lower_or_digit {
            snake.push('_');
        }
        snake.push(ch.to_ascii_lowercase());
        prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    snake
        .parse::<EdgeKind>()
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("{e}")))
}
