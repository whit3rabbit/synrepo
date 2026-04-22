use synrepo::pipeline::repair::{CommentaryWorkItem, CommentaryWorkPhase};

pub(super) fn render_target(item: &CommentaryWorkItem) -> String {
    match &item.qualified_name {
        Some(name) => format!("{} :: {}", item.path, name),
        None => item.path.clone(),
    }
}

pub(super) fn start_label(item: &CommentaryWorkItem) -> &'static str {
    match item.phase {
        CommentaryWorkPhase::Refresh => "Refreshing stale commentary",
        CommentaryWorkPhase::Seed => "Generating missing commentary",
    }
}

pub(super) fn success_label(phase: CommentaryWorkPhase) -> &'static str {
    match phase {
        CommentaryWorkPhase::Refresh => "refreshed",
        CommentaryWorkPhase::Seed => "generated",
    }
}

pub(super) fn phase_summary_label(phase: CommentaryWorkPhase) -> &'static str {
    match phase {
        CommentaryWorkPhase::Refresh => "Refresh phase",
        CommentaryWorkPhase::Seed => "Missing commentary phase",
    }
}

pub(super) fn provider_name(provider_label: &str) -> &str {
    provider_label.split(" / ").next().unwrap_or(provider_label)
}

pub(super) fn progress_label(attempted: usize, max_targets: usize, finished: bool) -> String {
    if finished {
        return format!("{attempted}/{max_targets} attempted, finishing output");
    }
    if max_targets == 0 {
        return "planning queued commentary work".to_string();
    }
    let percent = ((attempted as f64 / max_targets as f64) * 100.0).round() as usize;
    format!("{attempted}/{max_targets} attempted ({percent}%)")
}
