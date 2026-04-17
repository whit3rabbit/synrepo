//! Built-in dark palette plus a plain-rendering fallback for `--no-color` or
//! non-ANSI terminals. Widgets consume the semantic tokens (`healthy`,
//! `stale`, `blocked`, `watch_active`, `agent_accent`) rather than raw colors
//! so the visual language stays coherent across panes.

use ratatui::style::{Color, Modifier, Style};

/// Theme variant: full dark palette or plain (no color, no styling).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeVariant {
    /// Dark palette. Default.
    Dark,
    /// No color, no modifier. Honored via `--no-color` or when the terminal
    /// doesn't advertise color support.
    Plain,
}

/// Semantic color tokens + rendering modifiers used by dashboard widgets.
/// Always construct via [`Theme::dark`] or [`Theme::plain`].
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    /// Which variant this theme was built from; useful for tests.
    pub variant: ThemeVariant,
    /// Default foreground on panes.
    pub foreground: Color,
    /// Pane borders and chrome.
    pub border: Color,
    /// Dim foreground for muted lines (captions, labels).
    pub muted: Color,
    /// Healthy state (reconcile current, no compat action, watch running).
    pub healthy: Color,
    /// Stale state (export stale, commentary stale, reconcile stale).
    pub stale: Color,
    /// Blocked state (compat block, writer-lock contention, errors).
    pub blocked: Color,
    /// Watch-active accent.
    pub watch_active: Color,
    /// Agent / MCP accent.
    pub agent_accent: Color,
}

impl Theme {
    /// Dark palette used by the default dashboard rendering.
    pub fn dark() -> Self {
        Theme {
            variant: ThemeVariant::Dark,
            foreground: Color::Rgb(0xe0, 0xe0, 0xe0),
            border: Color::Rgb(0x60, 0x6a, 0x76),
            muted: Color::Rgb(0x98, 0xa0, 0xa8),
            healthy: Color::Rgb(0x5c, 0xd0, 0x7c),
            stale: Color::Rgb(0xe8, 0xa8, 0x3b),
            blocked: Color::Rgb(0xe0, 0x52, 0x58),
            watch_active: Color::Rgb(0x48, 0xc0, 0xd8),
            agent_accent: Color::Rgb(0xc8, 0x7c, 0xd0),
        }
    }

    /// Plain-terminal palette. Every token collapses to the terminal default so
    /// no ANSI color codes are emitted; widgets still distinguish states via
    /// prefix glyphs or text labels.
    pub fn plain() -> Self {
        Theme {
            variant: ThemeVariant::Plain,
            foreground: Color::Reset,
            border: Color::Reset,
            muted: Color::Reset,
            healthy: Color::Reset,
            stale: Color::Reset,
            blocked: Color::Reset,
            watch_active: Color::Reset,
            agent_accent: Color::Reset,
        }
    }

    /// Pick a theme from `no_color`.
    pub fn from_no_color(no_color: bool) -> Self {
        if no_color {
            Self::plain()
        } else {
            Self::dark()
        }
    }

    /// Style for a healthy signal.
    pub fn healthy_style(&self) -> Style {
        self.fg(self.healthy)
    }
    /// Style for a stale / warning signal.
    pub fn stale_style(&self) -> Style {
        self.fg(self.stale)
    }
    /// Style for a blocked / error signal.
    pub fn blocked_style(&self) -> Style {
        self.fg(self.blocked)
    }
    /// Style for a watch-active accent.
    pub fn watch_active_style(&self) -> Style {
        self.fg(self.watch_active)
    }
    /// Style for an agent / MCP accent.
    pub fn agent_style(&self) -> Style {
        self.fg(self.agent_accent)
    }
    /// Default foreground style.
    pub fn base_style(&self) -> Style {
        self.fg(self.foreground)
    }
    /// Muted style used for captions and secondary labels.
    pub fn muted_style(&self) -> Style {
        self.fg(self.muted)
    }
    /// Border style used by pane chrome.
    pub fn border_style(&self) -> Style {
        self.fg(self.border)
    }
    /// Style for a highlighted (selected) quick-action row.
    pub fn selected_style(&self) -> Style {
        match self.variant {
            ThemeVariant::Dark => Style::default()
                .fg(Color::Black)
                .bg(self.watch_active)
                .add_modifier(Modifier::BOLD),
            ThemeVariant::Plain => Style::default().add_modifier(Modifier::REVERSED),
        }
    }

    fn fg(&self, color: Color) -> Style {
        match self.variant {
            ThemeVariant::Dark => Style::default().fg(color),
            ThemeVariant::Plain => Style::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_no_color_maps_to_plain() {
        assert_eq!(Theme::from_no_color(true).variant, ThemeVariant::Plain);
        assert_eq!(Theme::from_no_color(false).variant, ThemeVariant::Dark);
    }

    #[test]
    fn plain_variant_emits_no_fg_color() {
        let t = Theme::plain();
        // All token styles should be equivalent to Style::default() — no fg set.
        assert_eq!(t.healthy_style(), Style::default());
        assert_eq!(t.stale_style(), Style::default());
        assert_eq!(t.blocked_style(), Style::default());
        assert_eq!(t.watch_active_style(), Style::default());
        assert_eq!(t.agent_style(), Style::default());
    }

    #[test]
    fn dark_variant_sets_foreground_tokens() {
        let t = Theme::dark();
        assert_ne!(t.healthy_style(), Style::default());
        assert_ne!(t.blocked_style(), Style::default());
    }
}
