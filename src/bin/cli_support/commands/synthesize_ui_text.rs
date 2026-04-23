use synrepo::pipeline::repair::{CommentaryWorkItem, CommentaryWorkPhase};

pub(super) fn render_target(item: &CommentaryWorkItem) -> String {
    match &item.qualified_name {
        Some(name) => format!("{} :: {}", item.path, name),
        None => item.path.clone(),
    }
}

pub(super) fn start_label(item: &CommentaryWorkItem) -> &'static str {
    match item.phase {
        CommentaryWorkPhase::Refresh => "Updating commentary for",
        CommentaryWorkPhase::Seed => "Generating commentary for",
    }
}

pub(super) fn success_label(phase: CommentaryWorkPhase) -> &'static str {
    match phase {
        CommentaryWorkPhase::Refresh => "Updated",
        CommentaryWorkPhase::Seed => "Generated",
    }
}

pub(super) fn phase_summary_label(phase: CommentaryWorkPhase) -> &'static str {
    match phase {
        CommentaryWorkPhase::Refresh => "Outdated items",
        CommentaryWorkPhase::Seed => "Missing commentary",
    }
}

pub(super) fn work_found_label(
    scoped_files: usize,
    scoped_symbols: usize,
    refresh: usize,
    files: usize,
    symbols: usize,
) -> String {
    let total = refresh + files + symbols;
    let checked = format!("Checked {scoped_files} file(s) and {scoped_symbols} symbol(s).");
    if total == 0 {
        return format!("{checked} Nothing in this scope needs new commentary.");
    }
    format!(
        "{checked} Found {total} item(s) that need work: {refresh} outdated, {files} files without commentary, {symbols} symbol candidate(s)"
    )
}

pub(super) fn scan_progress_label(
    files_scanned: usize,
    files_total: usize,
    symbols_scanned: usize,
    symbols_total: usize,
) -> String {
    if files_total == 0 && symbols_total == 0 {
        return "scanning repository for commentary work".to_string();
    }
    format!(
        "checked {files_scanned}/{files_total} files, {symbols_scanned}/{symbols_total} symbols"
    )
}

pub(super) fn scan_work_label(
    files_scanned: usize,
    files_total: usize,
    symbols_scanned: usize,
    symbols_total: usize,
) -> String {
    if files_total == 0 && symbols_total == 0 {
        return "Checking repository coverage.".to_string();
    }
    format!(
        "Checking repository coverage: {files_scanned}/{files_total} files, {symbols_scanned}/{symbols_total} symbols."
    )
}

pub(super) fn progress_label(
    attempted: usize,
    max_targets: usize,
    finished: bool,
    scoped_files: usize,
    scoped_symbols: usize,
) -> String {
    let checked = format!("checked {scoped_files} files, {scoped_symbols} symbols");
    if max_targets == 0 {
        return format!("{checked}; nothing needs commentary");
    }
    if finished {
        return format!("{checked}; {attempted}/{max_targets} queued items done");
    }
    format!("{checked}; {attempted}/{max_targets} queued items done")
}

pub(super) fn fit_value(value: &str, max_width: usize) -> String {
    if value.len() <= max_width {
        return value.to_string();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }
    let keep = max_width - 3;
    format!("{}...", &value[..keep])
}
