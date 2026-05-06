use super::{
    is_file_path, path_target_kind, push_searches, push_unique, scoped_paths, scoped_symbols,
};
use crate::surface::context::types::{ContextAskRequest, ContextTarget};

pub(super) fn plan(
    targets: &mut Vec<ContextTarget>,
    request: &ContextAskRequest,
    _budget: &str,
    notes: &mut Vec<String>,
) {
    if request.ground.allow_overlay {
        push_unique(targets, "findings", "all", Some("tiny"));
    } else {
        notes.push(
            "findings were excluded because overlay-backed audit context was not allowed"
                .to_string(),
        );
    }
    push_unique(
        targets,
        "recent_activity",
        "release_readiness",
        Some("tiny"),
    );
    for path in scoped_paths(request) {
        push_unique(targets, path_target_kind(path), path, Some("tiny"));
        if is_file_path(path) {
            push_unique(targets, "change_risk", path, Some("normal"));
        } else {
            push_unique(targets, "public_api", path, Some("normal"));
        }
    }
    for symbol in scoped_symbols(request) {
        push_unique(targets, "symbol", symbol, Some("tiny"));
        push_unique(targets, "change_risk", symbol, Some("normal"));
    }
    push_searches(
        targets,
        ["TODO", "FIXME", "panic!", "unwrap()", request.ask.trim()],
    );
}
