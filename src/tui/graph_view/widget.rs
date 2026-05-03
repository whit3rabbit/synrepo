use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    symbols,
    text::{Line as TextLine, Span},
    widgets::{
        canvas::{Canvas, Line as CanvasLine, Points},
        Block, Borders, Paragraph, Widget, Wrap,
    },
};

use crate::surface::graph_view::{GraphViewEdge, GraphViewNode};
use crate::tui::graph_view::GraphViewState;

pub(crate) struct GraphViewWidget<'a> {
    pub(crate) state: &'a GraphViewState,
}

impl Widget for GraphViewWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 60 || area.height < 12 {
            render_too_small(area, buf, self.state);
            return;
        }
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(8),
                Constraint::Length(1),
            ])
            .split(area);
        render_header(outer[0], buf, self.state);
        render_body(outer[1], buf, self.state);
        render_footer(outer[2], buf, self.state);
    }
}

fn render_header(area: Rect, buf: &mut Buffer, state: &GraphViewState) {
    let model = &state.model;
    let target = model
        .target
        .as_deref()
        .or(model.focal_node_id.as_deref())
        .unwrap_or("top-degree overview");
    let title = TextLine::from(vec![
        Span::styled("graph view", state.theme.agent_style()),
        Span::raw("  "),
        Span::styled(target, state.theme.base_style()),
    ]);
    let meta = format!(
        "{} nodes, {} edges, depth {}, {}, limit {}{}",
        model.counts.nodes,
        model.counts.edges,
        model.depth,
        model.direction,
        model.limit,
        if model.truncated { ", truncated" } else { "" }
    );
    Paragraph::new(vec![title, TextLine::from(meta)])
        .block(block(" neighborhood ", state))
        .render(area, buf);
}

fn render_body(area: Rect, buf: &mut Buffer, state: &GraphViewState) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28),
            Constraint::Percentage(44),
            Constraint::Percentage(28),
        ])
        .split(area);
    render_node_list(columns[0], buf, state);
    render_canvas(columns[1], buf, state);
    render_details(columns[2], buf, state);
}

fn render_node_list(area: Rect, buf: &mut Buffer, state: &GraphViewState) {
    let visible = state.visible_node_indices();
    let mut lines = Vec::new();
    if visible.is_empty() {
        lines.push(TextLine::styled(
            "no matching nodes",
            state.theme.muted_style(),
        ));
    }
    for (position, index) in visible.into_iter().enumerate() {
        let node = &state.model.nodes[index];
        let selected = position == state.selected;
        let marker = if selected { "> " } else { "  " };
        let style = if selected {
            state.theme.selected_style()
        } else {
            state.theme.base_style()
        };
        lines.push(TextLine::from(vec![
            Span::styled(marker, style),
            Span::styled(format!("{:<7}", node.node_type), state.theme.muted_style()),
            Span::styled(shorten(&node.label, 28), style),
        ]));
    }
    Paragraph::new(lines)
        .block(block(" nodes ", state))
        .wrap(Wrap { trim: true })
        .render(area, buf);
}

fn render_canvas(area: Rect, buf: &mut Buffer, state: &GraphViewState) {
    let selected = state
        .selected_node()
        .map(|node| node.id.clone())
        .or_else(|| state.model.focal_node_id.clone());
    let points = layout_points(state, selected.as_deref());
    let by_id = points
        .iter()
        .map(|point| (point.id.clone(), (point.x, point.y)))
        .collect::<HashMap<_, _>>();
    let segments = state
        .model
        .edges
        .iter()
        .filter_map(|edge| {
            let from = by_id.get(&edge.from)?;
            let to = by_id.get(&edge.to)?;
            Some((*from, *to, edge.kind.clone()))
        })
        .collect::<Vec<_>>();
    let coords = points
        .iter()
        .map(|point| (point.x, point.y))
        .collect::<Vec<_>>();
    let theme = state.theme;

    let canvas = Canvas::default()
        .block(block(" graph ", state))
        .marker(symbols::Marker::Dot)
        .x_bounds([0.0, 100.0])
        .y_bounds([0.0, 100.0])
        .paint(move |ctx| {
            for ((x1, y1), (x2, y2), _) in &segments {
                ctx.draw(&CanvasLine::new(*x1, *y1, *x2, *y2, theme.border));
            }
            ctx.draw(&Points::new(&coords, theme.watch_active));
            for point in &points {
                let style = if Some(point.id.as_str()) == selected.as_deref() {
                    theme.selected_style()
                } else {
                    theme.base_style()
                };
                ctx.print(
                    point.x + 1.0,
                    point.y,
                    Span::styled(point.label.clone(), style),
                );
            }
        });
    canvas.render(area, buf);
}

fn render_details(area: Rect, buf: &mut Buffer, state: &GraphViewState) {
    let mut lines = Vec::new();
    if let Some(node) = state.selected_node() {
        lines.push(TextLine::styled(
            shorten(&node.label, 36),
            state.theme.agent_style(),
        ));
        lines.push(TextLine::from(format!("id: {}", node.id)));
        lines.push(TextLine::from(format!("type: {}", node.node_type)));
        if let Some(path) = &node.path {
            lines.push(TextLine::from(format!("path: {path}")));
        }
        if let Some(file_id) = &node.file_id {
            lines.push(TextLine::from(format!("file: {file_id}")));
        }
        lines.push(TextLine::from(format!(
            "degree: {} in, {} out",
            node.degree.inbound, node.degree.outbound
        )));
        lines.push(TextLine::raw(""));
        lines.push(TextLine::styled("incident", state.theme.muted_style()));
        lines.extend(incident_lines(
            node,
            &state.model.edges,
            &state.model.nodes,
            state,
        ));
    } else {
        lines.push(TextLine::styled(
            "no node selected",
            state.theme.muted_style(),
        ));
    }
    Paragraph::new(lines)
        .block(block(" details ", state))
        .wrap(Wrap { trim: true })
        .render(area, buf);
}

fn incident_lines(
    node: &GraphViewNode,
    edges: &[GraphViewEdge],
    nodes: &[GraphViewNode],
    state: &GraphViewState,
) -> Vec<TextLine<'static>> {
    let labels = nodes
        .iter()
        .map(|node| (node.id.as_str(), node.label.as_str()))
        .collect::<HashMap<_, _>>();
    let mut lines = Vec::new();
    for edge in edges
        .iter()
        .filter(|edge| edge.from == node.id || edge.to == node.id)
    {
        let (arrow, peer) = if edge.from == node.id {
            ("->", edge.to.as_str())
        } else {
            ("<-", edge.from.as_str())
        };
        let label = labels.get(peer).copied().unwrap_or(peer);
        lines.push(TextLine::from(vec![
            Span::styled(format!("{arrow} "), state.theme.muted_style()),
            Span::styled(format!("{} ", edge.kind), state.theme.base_style()),
            Span::raw(shorten(label, 24)),
        ]));
    }
    if lines.is_empty() {
        lines.push(TextLine::styled("none in view", state.theme.muted_style()));
    }
    lines
}

fn render_footer(area: Rect, buf: &mut Buffer, state: &GraphViewState) {
    let text = if state.filter_mode {
        format!("/{}", state.filter)
    } else if let Some(toast) = &state.toast {
        format!("{toast}   arrows select  Enter refocus  +/- depth  i/o/b direction  / filter  q")
    } else {
        "arrows select  Enter refocus  +/- depth  i/o/b direction  / filter  q".to_string()
    };
    Paragraph::new(text)
        .style(state.theme.muted_style())
        .render(area, buf);
}

fn render_too_small(area: Rect, buf: &mut Buffer, state: &GraphViewState) {
    Paragraph::new("terminal too small for graph view")
        .style(state.theme.blocked_style())
        .block(block(" graph ", state))
        .render(area, buf);
}

fn block<'a>(title: &'a str, state: &GraphViewState) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(state.theme.border_style())
        .style(Style::default().fg(state.theme.foreground))
}

#[derive(Clone, Debug)]
struct NodePoint {
    id: String,
    label: String,
    x: f64,
    y: f64,
}

fn layout_points(state: &GraphViewState, selected: Option<&str>) -> Vec<NodePoint> {
    let visible = state.visible_node_indices();
    let mut nodes = visible
        .into_iter()
        .filter_map(|index| state.model.nodes.get(index))
        .take(18)
        .collect::<Vec<_>>();
    if let Some(selected) = selected {
        nodes.sort_by_key(|node| if node.id == selected { 0 } else { 1 });
    }
    let peers = nodes.len().saturating_sub(1).max(1);
    let mut points = Vec::new();
    for (index, node) in nodes.into_iter().enumerate() {
        let (x, y) = if index == 0 {
            (50.0, 50.0)
        } else {
            let angle = (index - 1) as f64 * std::f64::consts::TAU / peers as f64;
            (50.0 + angle.cos() * 34.0, 50.0 + angle.sin() * 34.0)
        };
        points.push(NodePoint {
            id: node.id.clone(),
            label: shorten(&node.label, 16),
            x,
            y,
        });
    }
    points
}

fn shorten(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let head = chars.by_ref().take(max).collect::<String>();
    if chars.next().is_some() {
        format!("{head}...")
    } else {
        head
    }
}
