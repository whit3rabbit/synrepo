//! Small result popup shared by post-apply wizard handoffs.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::tui::app::poll_key;
use crate::tui::theme::Theme;
use crate::tui::wizard::{enter_tui, leave_tui, WizardTerminal};

/// Run a dismissible result popup until Enter, Esc, q, or Ctrl-C.
pub fn run_result_popup_loop(theme: Theme, title: &str, lines: &[String]) -> anyhow::Result<()> {
    let mut terminal = enter_tui()?;
    let result = render_loop(&mut terminal, &theme, title, lines);
    leave_tui(&mut terminal)?;
    result
}

fn render_loop(
    terminal: &mut WizardTerminal,
    theme: &Theme,
    title: &str,
    lines: &[String],
) -> anyhow::Result<()> {
    use std::time::Duration;
    loop {
        terminal.draw(|frame| draw_result_popup(frame, theme, title, lines))?;
        if let Some((code, mods)) = poll_key(Duration::from_millis(250))? {
            if dismisses_popup(code, mods) {
                return Ok(());
            }
        }
    }
}

fn dismisses_popup(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q'))
        || (code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL))
}

fn draw_result_popup(frame: &mut ratatui::Frame, theme: &Theme, title: &str, lines: &[String]) {
    let size = frame.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(size);

    let title_text = format!(" {title} ");
    let header = Paragraph::new(Line::from(Span::styled(title_text, theme.agent_style()))).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style()),
    );
    frame.render_widget(header, outer[0]);
    draw_result_body(frame, outer[1], theme, lines);

    let footer = Paragraph::new(Span::styled(
        " Enter continue  Esc close ",
        theme.muted_style(),
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style()),
    );
    frame.render_widget(footer, outer[2]);
}

fn draw_result_body(frame: &mut ratatui::Frame, area: Rect, theme: &Theme, lines: &[String]) {
    let rendered: Vec<Line> = if lines.is_empty() {
        vec![Line::from(Span::styled(
            "No result details were reported.",
            theme.muted_style(),
        ))]
    } else {
        lines
            .iter()
            .map(|line| Line::from(Span::styled(line.clone(), theme.base_style())))
            .collect()
    };
    let body = Paragraph::new(rendered)
        .block(
            Block::default()
                .title(" result ")
                .borders(Borders::ALL)
                .border_style(theme.border_style()),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(body, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn enter_escape_q_and_ctrl_c_dismiss() {
        assert!(dismisses_popup(KeyCode::Enter, KeyModifiers::empty()));
        assert!(dismisses_popup(KeyCode::Esc, KeyModifiers::empty()));
        assert!(dismisses_popup(KeyCode::Char('q'), KeyModifiers::empty()));
        assert!(dismisses_popup(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!dismisses_popup(KeyCode::Char('x'), KeyModifiers::empty()));
    }

    #[test]
    fn draws_empty_and_long_result_lines() {
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let theme = Theme::plain();
        let lines = vec![
            "Shim: applied".to_string(),
            "A very long result line that should wrap inside the bordered result panel".to_string(),
        ];

        terminal
            .draw(|frame| draw_result_popup(frame, &theme, "integration complete", &lines))
            .expect("draw long lines");
        terminal
            .draw(|frame| draw_result_popup(frame, &theme, "integration complete", &[]))
            .expect("draw empty lines");
    }
}
