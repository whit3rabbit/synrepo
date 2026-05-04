//! Browser script chunks for the graph export.

mod details;
mod view;

pub(super) const HTML_SUFFIX: [&str; 2] = [view::HTML_SCRIPT_START, details::HTML_SCRIPT_END];
