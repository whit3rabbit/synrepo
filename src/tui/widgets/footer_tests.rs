use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use super::*;
use crate::tui::theme::Theme;

fn footer(
    active: ActiveTab,
    follow: bool,
    watch_toggle_label: Option<&str>,
) -> (Vec<HintGroup>, Theme) {
    let theme = Theme::plain();
    let widget = FooterWidget {
        active,
        follow_mode: follow,
        theme: &theme,
        toast: None,
        watch_toggle_label,
        materialize_hint_visible: false,
    };
    (widget.build_hints(), theme)
}

fn footer_with_materialize(
    active: ActiveTab,
    watch_toggle_label: Option<&str>,
    materialize_hint_visible: bool,
) -> Vec<HintGroup> {
    let theme = Theme::plain();
    let widget = FooterWidget {
        active,
        follow_mode: false,
        theme: &theme,
        toast: None,
        watch_toggle_label,
        materialize_hint_visible,
    };
    widget.build_hints()
}

fn rendered_text(spans: &[Span<'static>]) -> String {
    spans.iter().map(|s| s.content.as_ref()).collect()
}

#[test]
fn wide_terminal_keeps_every_hint() {
    let (groups, _) = footer(ActiveTab::Live, true, Some("stop"));
    let spans = fit_groups(groups, 200);
    let text = rendered_text(&spans);
    assert!(text.contains("scroll"));
    assert!(text.contains("follow"));
    assert!(text.contains("[:]") && text.contains("[Tab/1-8]"));
    assert!(!text.contains("watch"));
    assert!(!text.contains("integration"));
    assert!(text.ends_with("[q]"));
}

#[test]
fn narrow_terminal_preserves_quit_hint() {
    for active in [ActiveTab::Live, ActiveTab::Health, ActiveTab::Actions] {
        for width in [40usize, 50, 60, 70, 80] {
            let (groups, _) = footer(active, false, Some("start"));
            let spans = fit_groups(groups, width);
            let text = rendered_text(&spans);
            assert!(
                text.contains("[p]")
                    && text.contains("[?]")
                    && text.contains("[:]")
                    && text.contains("[q]"),
                "tab={active:?} width={width} must keep project/help/commands/quit, got {text:?}"
            );
            let visible: usize = spans.iter().map(|s| s.width()).sum();
            assert!(
                visible <= width,
                "tab={active:?} width={width} overflowed: visible={visible} text={text:?}"
            );
        }
    }
}

#[test]
fn narrow_live_drops_scroll_before_follow() {
    let (groups, _) = footer(ActiveTab::Live, true, Some("stop"));
    let spans = fit_groups(groups, 80);
    let text = rendered_text(&spans);
    assert!(
        !text.contains("scroll"),
        "scroll should drop first: {text:?}"
    );
    assert!(
        text.contains("follow"),
        "follow should survive at 80 cols: {text:?}"
    );
}

#[test]
fn explain_tab_shows_explain_and_docs_hints_when_wide() {
    let (groups, _) = footer(ActiveTab::Explain, false, None);
    let spans = fit_groups(groups, 200);
    let text = rendered_text(&spans);
    assert!(text.contains("[r/a/c/f]"), "missing explain hint: {text:?}");
    assert!(text.contains("[d/D/x/X]"), "missing docs hint: {text:?}");
}

#[test]
fn explain_tab_drops_explain_hints_on_narrow_terminal() {
    let (groups, _) = footer(ActiveTab::Explain, false, None);
    let spans = fit_groups(groups, 50);
    let text = rendered_text(&spans);
    assert!(
        text.contains("[p]")
            && text.contains("[?]")
            && text.contains("[:]")
            && text.contains("[q]")
    );
    assert!(
        !text.contains("[r/a/c/f]") && !text.contains("[d/D/x/X]"),
        "explain+docs hints should drop together on narrow: {text:?}"
    );
}

#[test]
fn explain_hints_survive_together_at_120_cols() {
    let (groups, _) = footer(ActiveTab::Explain, false, Some("stop"));
    let spans = fit_groups(groups, 120);
    let text = rendered_text(&spans);
    let has_run = text.contains("[r/a/c/f]");
    let has_docs = text.contains("[d/D/x/X]");
    assert_eq!(
        has_run, has_docs,
        "explain run and docs hints must drop together: {text:?}"
    );
}

#[test]
fn other_tabs_do_not_show_explain_hints() {
    for active in [ActiveTab::Live, ActiveTab::Health, ActiveTab::Actions] {
        let (groups, _) = footer(active, false, None);
        let spans = fit_groups(groups, 200);
        let text = rendered_text(&spans);
        assert!(
            !text.contains("[r/c/f]") && !text.contains("[d/D/x/X]"),
            "tab={active:?} leaked explain hint: {text:?}"
        );
    }
}

#[test]
fn toast_keeps_essential_hints_when_set() {
    let theme = Theme::plain();
    let widget = FooterWidget {
        active: ActiveTab::Live,
        follow_mode: false,
        theme: &theme,
        toast: Some("refreshed: 12 files, 34 symbols"),
        watch_toggle_label: Some("stop"),
        materialize_hint_visible: false,
    };
    let area = Rect::new(0, 0, 80, 1);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);
    let row: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
    assert!(
        row.contains("refreshed: 12 files"),
        "toast text must appear, even if truncated: {row:?}"
    );
    assert!(
        row.contains("[p]") && row.contains("[?]") && row.contains("[:]") && row.contains("[q]"),
        "toast must keep essential hints: {row:?}"
    );
}

#[test]
fn very_narrow_terminal_keeps_only_essentials() {
    let (groups, _) = footer(ActiveTab::Live, false, Some("start"));
    let spans = fit_groups(groups, 30);
    let text = rendered_text(&spans);
    assert!(text.contains("[p]"));
    assert!(text.contains("[?]"));
    assert!(text.contains("[:]"));
    assert!(text.contains("[q]"));
    assert!(!text.contains("tabs"));
    assert!(!text.contains("scroll"));
    assert!(!text.contains("follow"));
    assert!(!text.contains("integration"));
    assert!(!text.contains("watch"));
    assert!(!text.contains("refresh"));
    let visible: usize = spans.iter().map(|s| s.width()).sum();
    assert!(visible <= 30, "visible={visible} text={text:?}");
}

#[test]
fn omits_watch_hint_when_toggle_is_unavailable() {
    let (groups, _) = footer(ActiveTab::Health, false, None);
    let spans = fit_groups(groups, 200);
    let text = rendered_text(&spans);
    assert!(!text.contains("watch"));
}

#[test]
fn omits_materialize_hint_even_when_graph_missing() {
    let groups = footer_with_materialize(ActiveTab::Health, None, true);
    let spans = fit_groups(groups, 200);
    let text = rendered_text(&spans);
    assert!(!text.contains("[M]"), "stray [M] hint: {text:?}");
}

#[test]
fn omits_materialize_hint_when_graph_present() {
    let groups = footer_with_materialize(ActiveTab::Health, None, false);
    let spans = fit_groups(groups, 200);
    let text = rendered_text(&spans);
    assert!(!text.contains("[M]"), "stray [M] hint: {text:?}");
}
