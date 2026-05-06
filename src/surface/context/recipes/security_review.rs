use super::{
    is_file_path, path_target_kind, push_searches, push_unique, scoped_paths, scoped_symbols,
};
use crate::surface::context::types::{ContextAskRequest, ContextTarget};

pub(super) fn plan(targets: &mut Vec<ContextTarget>, request: &ContextAskRequest, budget: &str) {
    if request.scope.paths.is_empty() {
        push_unique(targets, "entrypoints", ".", Some("normal"));
    }
    for path in scoped_paths(request) {
        push_unique(targets, "entrypoints", path, Some("normal"));
        push_unique(targets, path_target_kind(path), path, Some("tiny"));
        if is_file_path(path) {
            push_unique(targets, "change_risk", path, Some("normal"));
        }
    }
    for symbol in scoped_symbols(request) {
        push_unique(targets, "symbol", symbol, Some(budget));
        push_unique(targets, "minimum_context", symbol, Some("tiny"));
        push_unique(targets, "change_risk", symbol, Some("normal"));
    }
    push_searches(
        targets,
        [
            "Command::new",
            "std::process",
            "std::fs",
            "File::create",
            "TcpStream",
            "reqwest",
            "auth",
            "crypto",
            "unsafe",
            request.ask.trim(),
        ],
    );
}
