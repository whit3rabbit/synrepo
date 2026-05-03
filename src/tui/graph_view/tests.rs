use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
use time::OffsetDateTime;

use super::widget::GraphViewWidget;
use super::GraphViewState;
use crate::core::provenance::{CreatedBy, Provenance, SourceRef};
use crate::structure::graph::Epistemic;
use crate::surface::graph_view::{
    GraphNeighborhood, GraphNeighborhoodRequest, GraphViewCounts, GraphViewDegree, GraphViewEdge,
    GraphViewNode,
};
use crate::tui::theme::Theme;

#[test]
fn graph_view_widget_renders_selected_node_details() {
    let state = GraphViewState {
        request: GraphNeighborhoodRequest::default(),
        model: sample_model(),
        theme: Theme::plain(),
        selected: 0,
        filter: String::new(),
        filter_mode: false,
        toast: None,
        should_exit: false,
    };
    let area = Rect::new(0, 0, 100, 24);
    let mut buf = Buffer::empty(area);

    GraphViewWidget { state: &state }.render(area, &mut buf);

    let rendered = rendered_text(&buf, area);
    assert!(rendered.contains("graph view"));
    assert!(rendered.contains("synrepo::lib"));
    assert!(rendered.contains("degree: 1 in, 0 out"));
    assert!(rendered.contains("governs"));
}

fn sample_model() -> GraphNeighborhood {
    let nodes = vec![
        GraphViewNode {
            id: "sym_0000000000000024".to_string(),
            node_type: "symbol",
            label: "synrepo::lib".to_string(),
            path: Some("src/lib.rs".to_string()),
            file_id: Some("file_0000000000000042".to_string()),
            degree: GraphViewDegree {
                inbound: 1,
                outbound: 0,
                total: 1,
            },
        },
        GraphViewNode {
            id: "file_0000000000000042".to_string(),
            node_type: "file",
            label: "src/lib.rs".to_string(),
            path: Some("src/lib.rs".to_string()),
            file_id: None,
            degree: GraphViewDegree {
                inbound: 0,
                outbound: 1,
                total: 1,
            },
        },
    ];
    let edges = vec![GraphViewEdge {
        id: "edge_0000000000000077".to_string(),
        from: "file_0000000000000042".to_string(),
        to: "sym_0000000000000024".to_string(),
        kind: "governs".to_string(),
        drift_score: 0.0,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance(),
    }];
    GraphNeighborhood {
        target: Some("synrepo::lib".to_string()),
        focal_node_id: Some("sym_0000000000000024".to_string()),
        direction: "both",
        depth: 1,
        limit: 100,
        edge_types: Vec::new(),
        counts: GraphViewCounts {
            nodes: nodes.len(),
            edges: edges.len(),
            files: 1,
            symbols: 1,
            concepts: 0,
            edges_by_kind: [("governs".to_string(), 1)].into_iter().collect(),
        },
        truncated: false,
        nodes,
        edges,
        source_store: "graph",
    }
}

fn sample_provenance() -> Provenance {
    Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "rev".to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: "parse_code".to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: "src/lib.rs".to_string(),
            content_hash: "hash".to_string(),
        }],
    }
}

fn rendered_text(buf: &Buffer, area: Rect) -> String {
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
