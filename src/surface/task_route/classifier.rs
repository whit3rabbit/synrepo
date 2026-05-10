use std::path::Path;

use super::routes::{route, with_edit_candidate, with_signals, with_strategy};
use super::{
    semantic, EditCandidate, TaskRoute, SIGNAL_CONTEXT_FAST_PATH,
    SIGNAL_DETERMINISTIC_EDIT_CANDIDATE, SIGNAL_LLM_NOT_REQUIRED,
};

/// Classify a plain-language task into the cheapest safe synrepo route.
pub fn classify_task_route(task: &str, path: Option<&str>) -> TaskRoute {
    let text = task.to_ascii_lowercase();
    if unsupported_semantic_transform(&text) {
        return route(
            "llm-required",
            0.86,
            &["synrepo_orient", "synrepo_ask", "synrepo_minimum_context"],
            "normal",
            true,
            "task requires semantic transformation beyond deterministic synrepo proof",
        );
    }

    if let Some(intent) = edit_intent(&text) {
        let candidate = EditCandidate {
            intent: intent.to_string(),
            eligible: false,
            reason: "prepare anchors and inspect source before applying any edit".to_string(),
        };
        return with_signals(
            with_edit_candidate(
                route(
                    intent,
                    edit_confidence(intent, path),
                    &[
                        "synrepo_orient",
                        "synrepo_find",
                        "synrepo_prepare_edit_context",
                        "synrepo_apply_anchor_edits",
                        "synrepo_changed",
                    ],
                    "normal",
                    false,
                    "mechanical edit candidate; source mutation remains gated by anchored edits",
                ),
                candidate,
            ),
            &[
                SIGNAL_CONTEXT_FAST_PATH,
                SIGNAL_DETERMINISTIC_EDIT_CANDIDATE,
                SIGNAL_LLM_NOT_REQUIRED,
            ],
        );
    }

    if has_any(&text, &["test", "tests", "coverage"]) {
        return with_signals(
            route(
                "test-surface",
                0.82,
                &["synrepo_orient", "synrepo_tests", "synrepo_risks"],
                "tiny",
                false,
                "test discovery is available from structural and path-convention context",
            ),
            &[SIGNAL_CONTEXT_FAST_PATH, SIGNAL_LLM_NOT_REQUIRED],
        );
    }

    if has_any(
        &text,
        &[
            "risk",
            "impact",
            "break",
            "depend",
            "review",
            "audit",
            "security",
            "threat model",
            "vulnerab",
            "refactor",
        ],
    ) {
        return with_signals(
            route(
                "risk-review",
                0.78,
                &[
                    "synrepo_orient",
                    "synrepo_ask",
                    "synrepo_find",
                    "synrepo_minimum_context",
                    "synrepo_risks",
                    "synrepo_tests",
                ],
                "normal",
                false,
                "review and risk tasks should start with graph-backed synthesis before broad find",
            ),
            &[SIGNAL_CONTEXT_FAST_PATH, SIGNAL_LLM_NOT_REQUIRED],
        );
    }

    if has_any(
        &text,
        &[
            "architecture",
            "architectural",
            "design",
            "compare",
            "proposal",
            "proposed",
            "improvement",
            "improvements",
        ],
    ) {
        return with_signals(
            route(
                "broad-context-question",
                0.74,
                &[
                    "synrepo_orient",
                    "synrepo_ask",
                    "synrepo_search(output_mode=\"compact\")",
                    "synrepo_minimum_context",
                ],
                "normal",
                true,
                "broad architecture questions need synthesized graph context before exact searches",
            ),
            &[SIGNAL_CONTEXT_FAST_PATH],
        );
    }

    if has_any(
        &text,
        &[
            "search", "find", "where", "read", "symbol", "file", "module", "codebase", "repo",
        ],
    ) {
        return with_signals(
            route(
                "context-search",
                0.76,
                &[
                    "synrepo_orient",
                    "synrepo_search(output_mode=\"compact\")",
                    "synrepo_find",
                    "synrepo_context_pack(output_mode=\"compact\")",
                ],
                "tiny",
                false,
                "compact search and cards are cheaper than cold source reads",
            ),
            &[SIGNAL_CONTEXT_FAST_PATH, SIGNAL_LLM_NOT_REQUIRED],
        );
    }

    route(
        "general",
        0.35,
        &["synrepo_orient", "synrepo_find"],
        "tiny",
        true,
        "no deterministic fast path matched confidently",
    )
}

/// Classify a task, using local semantic routing when it is available.
pub fn classify_task_route_with_config(
    task: &str,
    path: Option<&str>,
    config: &crate::config::Config,
    synrepo_dir: &Path,
) -> TaskRoute {
    let text = task.to_ascii_lowercase();
    let keyword = classify_task_route(task, path);
    if unsupported_semantic_transform(&text) || edit_intent(&text).is_some() {
        return keyword;
    }

    if let Some(semantic_match) = semantic::classify(task, config, synrepo_dir) {
        if semantic_match.score >= config.semantic_similarity_threshold as f32 {
            if let Some(route) = route_for_semantic_intent(&semantic_match.intent, path) {
                return with_strategy(route, "semantic", Some(semantic_match.score));
            }
        }
        return with_strategy(keyword, "keyword_fallback", Some(semantic_match.score));
    }

    keyword
}

fn unsupported_semantic_transform(text: &str) -> bool {
    has_any(
        text,
        &[
            "add type",
            "add types",
            "type annotation",
            "typescript type",
            "try/catch",
            "try catch",
            "error handling",
            "async await",
            "async/await",
            ".then",
            "promise chain",
        ],
    )
}

fn edit_intent(text: &str) -> Option<&'static str> {
    if has_any(text, &["var to const", "var-to-const", "let to const"]) {
        Some("var-to-const")
    } else if has_any(
        text,
        &[
            "remove console",
            "strip console",
            "remove debug log",
            "debug logging",
        ],
    ) {
        Some("remove-debug-logging")
    } else if has_any(
        text,
        &[
            "replace literal",
            "replace string literal",
            "change literal",
        ],
    ) {
        Some("replace-literal")
    } else if has_any(text, &["rename local", "local rename"]) {
        Some("rename-local")
    } else {
        None
    }
}

fn edit_confidence(intent: &str, path: Option<&str>) -> f32 {
    match (intent, path.and_then(file_ext)) {
        ("var-to-const", Some("ts" | "tsx")) => 0.82,
        ("var-to-const", _) => 0.64,
        ("remove-debug-logging", Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx")) => 0.74,
        ("replace-literal" | "rename-local", _) => 0.68,
        _ => 0.55,
    }
}

fn file_ext(path: &str) -> Option<&str> {
    path.rsplit_once('.').map(|(_, ext)| ext)
}

fn route_for_semantic_intent(intent: &str, path: Option<&str>) -> Option<TaskRoute> {
    match intent {
        "var-to-const" | "remove-debug-logging" | "replace-literal" | "rename-local" => {
            let candidate = EditCandidate {
                intent: intent.to_string(),
                eligible: false,
                reason: "semantic routing matched a mechanical edit intent; source mutation remains gated by anchored edits".to_string(),
            };
            Some(with_signals(
                with_edit_candidate(
                    route(
                        intent,
                        edit_confidence(intent, path).max(0.7),
                        &[
                            "synrepo_orient",
                            "synrepo_ask",
                            "synrepo_find",
                            "synrepo_prepare_edit_context",
                            "synrepo_apply_anchor_edits",
                            "synrepo_changed",
                        ],
                        "normal",
                        false,
                        "semantic routing matched a mechanical edit candidate",
                    ),
                    candidate,
                ),
                &[
                    SIGNAL_CONTEXT_FAST_PATH,
                    SIGNAL_DETERMINISTIC_EDIT_CANDIDATE,
                    SIGNAL_LLM_NOT_REQUIRED,
                ],
            ))
        }
        "test-surface" => Some(with_signals(
            route(
                "test-surface",
                0.82,
                &["synrepo_orient", "synrepo_tests", "synrepo_risks"],
                "tiny",
                false,
                "semantic routing matched a test-discovery task",
            ),
            &[SIGNAL_CONTEXT_FAST_PATH, SIGNAL_LLM_NOT_REQUIRED],
        )),
        "risk-review" => Some(with_signals(
            route(
                "risk-review",
                0.78,
                &[
                    "synrepo_orient",
                    "synrepo_ask",
                    "synrepo_find",
                    "synrepo_minimum_context",
                    "synrepo_risks",
                    "synrepo_tests",
                ],
                "normal",
                false,
                "semantic routing matched a review or change-risk task",
            ),
            &[SIGNAL_CONTEXT_FAST_PATH, SIGNAL_LLM_NOT_REQUIRED],
        )),
        "context-search" => Some(with_signals(
            route(
                "context-search",
                0.76,
                &[
                    "synrepo_orient",
                    "synrepo_search(output_mode=\"compact\")",
                    "synrepo_find",
                    "synrepo_context_pack(output_mode=\"compact\")",
                ],
                "tiny",
                false,
                "semantic routing matched a context-search task",
            ),
            &[SIGNAL_CONTEXT_FAST_PATH, SIGNAL_LLM_NOT_REQUIRED],
        )),
        _ => None,
    }
}

fn has_any(text: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| text.contains(term))
}
