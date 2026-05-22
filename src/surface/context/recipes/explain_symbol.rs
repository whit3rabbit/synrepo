use super::{is_file_path, path_target_kind, push_unique, scoped_paths, scoped_symbols};
use crate::surface::context::types::{ContextAskRequest, ContextTarget};

pub(super) fn plan(targets: &mut Vec<ContextTarget>, request: &ContextAskRequest, budget: &str) {
    for symbol in scoped_symbols(request) {
        push_unique(targets, "symbol", symbol, Some(budget));
        push_unique(targets, "minimum_context", symbol, Some("tiny"));
        push_unique(targets, "call_path", symbol, Some("tiny"));
        push_unique(targets, "source_slice", symbol, Some("normal"));
    }
    for path in scoped_paths(request) {
        push_unique(targets, path_target_kind(path), path, Some(budget));
        if is_file_path(path) {
            push_unique(targets, "source_slice", path, Some("normal"));
        }
    }
    if targets.is_empty() {
        push_unique(targets, "search", request.ask.trim(), Some("tiny"));
    }
}
