//! Terminal graph-neighborhood explorer.
//!
//! This view is intentionally runtime-only: it renders the bounded
//! graph-neighborhood model from `surface::graph_view` and never writes graph
//! facts or feeds explain input.

mod widget;

#[cfg(test)]
mod tests;

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::{
    event::{KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::surface::graph_view::{
    GraphNeighborhood, GraphNeighborhoodRequest, GraphViewDirection, GraphViewNode,
};
use crate::tui::app::poll_key;
use crate::tui::graph_view::widget::GraphViewWidget;
use crate::tui::theme::Theme;

type GraphTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Run the terminal graph view with a caller-provided model loader.
pub fn run_graph_view(
    initial: GraphNeighborhoodRequest,
    theme: Theme,
    load: &mut dyn FnMut(GraphNeighborhoodRequest) -> anyhow::Result<GraphNeighborhood>,
) -> anyhow::Result<()> {
    let mut terminal = enter_tui()?;
    let mut state = GraphViewState::new(initial, theme, load)?;
    let result = render_loop(&mut terminal, &mut state, load);
    leave_tui(&mut terminal)?;
    result
}

#[derive(Clone, Debug)]
pub(crate) struct GraphViewState {
    pub(crate) request: GraphNeighborhoodRequest,
    pub(crate) model: GraphNeighborhood,
    pub(crate) theme: Theme,
    pub(crate) selected: usize,
    pub(crate) filter: String,
    pub(crate) filter_mode: bool,
    pub(crate) toast: Option<String>,
    should_exit: bool,
}

impl GraphViewState {
    fn new(
        request: GraphNeighborhoodRequest,
        theme: Theme,
        load: &mut dyn FnMut(GraphNeighborhoodRequest) -> anyhow::Result<GraphNeighborhood>,
    ) -> anyhow::Result<Self> {
        let model = load(request.clone())?;
        let mut state = Self {
            request,
            model,
            theme,
            selected: 0,
            filter: String::new(),
            filter_mode: false,
            toast: None,
            should_exit: false,
        };
        state.select_focal();
        Ok(state)
    }

    pub(crate) fn visible_node_indices(&self) -> Vec<usize> {
        let needle = self.filter.trim().to_ascii_lowercase();
        self.model
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                if needle.is_empty() || node_matches(node, &needle) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn selected_node(&self) -> Option<&GraphViewNode> {
        let visible = self.visible_node_indices();
        visible
            .get(self.selected)
            .and_then(|index| self.model.nodes.get(*index))
    }

    fn handle_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
        load: &mut dyn FnMut(GraphNeighborhoodRequest) -> anyhow::Result<GraphNeighborhood>,
    ) -> anyhow::Result<()> {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.should_exit = true;
            return Ok(());
        }
        if self.filter_mode {
            return self.handle_filter_key(code);
        }
        match code {
            KeyCode::Esc | KeyCode::Char('q') => self.should_exit = true,
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Enter => self.refocus(load)?,
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.set_depth(self.request.depth + 1, load)?
            }
            KeyCode::Char('-') => self.set_depth(self.request.depth.saturating_sub(1), load)?,
            KeyCode::Char('i') => self.set_direction(GraphViewDirection::Inbound, load)?,
            KeyCode::Char('o') => self.set_direction(GraphViewDirection::Outbound, load)?,
            KeyCode::Char('b') => self.set_direction(GraphViewDirection::Both, load)?,
            KeyCode::Char('/') => {
                self.filter_mode = true;
                self.toast = Some("filter".to_string());
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_filter_key(&mut self, code: KeyCode) -> anyhow::Result<()> {
        match code {
            KeyCode::Esc | KeyCode::Enter => {
                self.filter_mode = false;
                self.toast = None;
                self.clamp_selection();
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.clamp_selection();
            }
            KeyCode::Char(ch) => {
                if !ch.is_control() {
                    self.filter.push(ch);
                    self.clamp_selection();
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn select_next(&mut self) {
        let len = self.visible_node_indices().len();
        if self.selected + 1 < len {
            self.selected += 1;
        }
    }

    fn set_depth(
        &mut self,
        depth: usize,
        load: &mut dyn FnMut(GraphNeighborhoodRequest) -> anyhow::Result<GraphNeighborhood>,
    ) -> anyhow::Result<()> {
        self.request.depth = depth.clamp(1, 3);
        self.reload(load, self.selected_node().map(|node| node.id.clone()))?;
        self.toast = Some(format!("depth {}", self.model.depth));
        Ok(())
    }

    fn set_direction(
        &mut self,
        direction: GraphViewDirection,
        load: &mut dyn FnMut(GraphNeighborhoodRequest) -> anyhow::Result<GraphNeighborhood>,
    ) -> anyhow::Result<()> {
        self.request.direction = direction;
        self.reload(load, self.selected_node().map(|node| node.id.clone()))?;
        self.toast = Some(format!("direction {}", self.model.direction));
        Ok(())
    }

    fn refocus(
        &mut self,
        load: &mut dyn FnMut(GraphNeighborhoodRequest) -> anyhow::Result<GraphNeighborhood>,
    ) -> anyhow::Result<()> {
        let Some(node_id) = self.selected_node().map(|node| node.id.clone()) else {
            return Ok(());
        };
        self.request.target = Some(node_id.clone());
        self.reload(load, Some(node_id))?;
        self.toast = Some("refocused".to_string());
        Ok(())
    }

    fn reload(
        &mut self,
        load: &mut dyn FnMut(GraphNeighborhoodRequest) -> anyhow::Result<GraphNeighborhood>,
        preferred_id: Option<String>,
    ) -> anyhow::Result<()> {
        self.model = load(self.request.clone())?;
        self.selected = 0;
        if let Some(id) = preferred_id.or_else(|| self.model.focal_node_id.clone()) {
            self.select_id(&id);
        }
        self.clamp_selection();
        Ok(())
    }

    fn select_focal(&mut self) {
        if let Some(id) = self.model.focal_node_id.clone() {
            self.select_id(&id);
        }
    }

    fn select_id(&mut self, id: &str) {
        let visible = self.visible_node_indices();
        if let Some(position) = visible
            .iter()
            .position(|index| self.model.nodes[*index].id == id)
        {
            self.selected = position;
        }
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_node_indices().len();
        self.selected = self.selected.min(len.saturating_sub(1));
    }
}

fn render_loop(
    terminal: &mut GraphTerminal,
    state: &mut GraphViewState,
    load: &mut dyn FnMut(GraphNeighborhoodRequest) -> anyhow::Result<GraphNeighborhood>,
) -> anyhow::Result<()> {
    while !state.should_exit {
        terminal.draw(|frame| {
            let widget = GraphViewWidget { state };
            frame.render_widget(widget, frame.area());
        })?;
        if let Some((code, mods)) = poll_key(Duration::from_millis(125))? {
            state.handle_key(code, mods, load)?;
        }
    }
    Ok(())
}

fn enter_tui() -> anyhow::Result<GraphTerminal> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

fn leave_tui(terminal: &mut GraphTerminal) -> anyhow::Result<()> {
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    Ok(())
}

fn node_matches(node: &GraphViewNode, needle: &str) -> bool {
    node.id.to_ascii_lowercase().contains(needle)
        || node.label.to_ascii_lowercase().contains(needle)
        || node
            .path
            .as_ref()
            .map(|path| path.to_ascii_lowercase().contains(needle))
            .unwrap_or(false)
}
