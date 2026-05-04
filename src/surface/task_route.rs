//! Deterministic task routing recommendations for agent fast paths.

use serde::{Deserialize, Serialize};

/// Stable hook signal for context-first routing.
pub const SIGNAL_CONTEXT_FAST_PATH: &str = "[SYNREPO_CONTEXT_FAST_PATH]";
/// Stable hook signal for deterministic edit candidates.
pub const SIGNAL_DETERMINISTIC_EDIT_CANDIDATE: &str =
    "[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE]";
/// Stable hook signal for work that does not need LLM output.
pub const SIGNAL_LLM_NOT_REQUIRED: &str = "[SYNREPO_LLM_NOT_REQUIRED]";

/// Result returned by the task-route classifier.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskRoute {
    /// Stable intent name, for example `context-search` or `var-to-const`.
    pub intent: String,
    /// Deterministic confidence score in the range 0.0..=1.0.
    pub confidence: f32,
    /// Recommended synrepo tools in the order an agent should try them.
    pub recommended_tools: Vec<String>,
    /// Recommended card budget tier for the first context read.
    pub budget_tier: String,
    /// True when the task needs semantic generation beyond structural context.
    pub llm_required: bool,
    /// Optional deterministic edit candidate. Advisory only.
    pub edit_candidate: Option<EditCandidate>,
    /// Stable signals suitable for hook output.
    pub signals: Vec<String>,
    /// Short human-readable explanation.
    pub reason: String,
}

/// Advisory deterministic edit candidate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EditCandidate {
    /// Candidate intent.
    pub intent: String,
    /// Whether eligibility was proven from supplied source text.
    pub eligible: bool,
    /// Why the candidate is or is not eligible.
    pub reason: String,
}

/// TypeScript `var`/`let` to `const` eligibility result.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VarToConstEligibility {
    /// True when a single var/let binding was found and no reassignment was observed.
    pub eligible: bool,
    /// Binding name, when a single simple binding was found.
    pub binding: Option<String>,
    /// Explanation of the decision.
    pub reason: String,
}

/// Classify a plain-language task into the cheapest safe synrepo route.
pub fn classify_task_route(task: &str, path: Option<&str>) -> TaskRoute {
    let text = task.to_ascii_lowercase();
    if unsupported_semantic_transform(&text) {
        return route(
            "llm-required",
            0.86,
            &["synrepo_orient", "synrepo_find", "synrepo_minimum_context"],
            "normal",
            true,
            None,
            &[],
            "task requires semantic transformation beyond deterministic synrepo proof",
        );
    }

    if let Some(intent) = edit_intent(&text) {
        let candidate = EditCandidate {
            intent: intent.to_string(),
            eligible: false,
            reason: "prepare anchors and inspect source before applying any edit".to_string(),
        };
        return route(
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
            Some(candidate),
            &[
                SIGNAL_CONTEXT_FAST_PATH,
                SIGNAL_DETERMINISTIC_EDIT_CANDIDATE,
                SIGNAL_LLM_NOT_REQUIRED,
            ],
            "mechanical edit candidate; source mutation remains gated by anchored edits",
        );
    }

    if has_any(&text, &["test", "tests", "coverage"]) {
        return route(
            "test-surface",
            0.82,
            &["synrepo_orient", "synrepo_tests", "synrepo_risks"],
            "tiny",
            false,
            None,
            &[SIGNAL_CONTEXT_FAST_PATH, SIGNAL_LLM_NOT_REQUIRED],
            "test discovery is available from structural and path-convention context",
        );
    }

    if has_any(&text, &["risk", "impact", "break", "depend", "review", "audit"]) {
        return route(
            "risk-review",
            0.78,
            &[
                "synrepo_orient",
                "synrepo_find",
                "synrepo_minimum_context",
                "synrepo_risks",
                "synrepo_tests",
            ],
            "normal",
            false,
            None,
            &[SIGNAL_CONTEXT_FAST_PATH, SIGNAL_LLM_NOT_REQUIRED],
            "review and risk tasks should start with graph-backed context",
        );
    }

    if has_any(
        &text,
        &["search", "find", "where", "read", "symbol", "file", "module", "codebase", "repo"],
    ) {
        return route(
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
            None,
            &[SIGNAL_CONTEXT_FAST_PATH, SIGNAL_LLM_NOT_REQUIRED],
            "compact search and cards are cheaper than cold source reads",
        );
    }

    route(
        "general",
        0.35,
        &["synrepo_orient", "synrepo_find"],
        "tiny",
        true,
        None,
        &[],
        "no deterministic fast path matched confidently",
    )
}

/// Check whether a TypeScript/TSX snippet can safely convert a single var/let to const.
pub fn typescript_var_to_const_eligibility(source: &str, tsx: bool) -> VarToConstEligibility {
    let language: tree_sitter::Language = if tsx {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    };
    let mut parser = tree_sitter::Parser::new();
    if let Err(error) = parser.set_language(&language) {
        return VarToConstEligibility {
            eligible: false,
            binding: None,
            reason: format!("failed to initialize TypeScript parser: {error}"),
        };
    }
    let Some(tree) = parser.parse(source.as_bytes(), None) else {
        return VarToConstEligibility {
            eligible: false,
            binding: None,
            reason: "source could not be parsed".to_string(),
        };
    };
    let root = tree.root_node();
    if root.has_error() {
        return VarToConstEligibility {
            eligible: false,
            binding: None,
            reason: "source contains parse errors".to_string(),
        };
    }

    let mut declarations = Vec::new();
    collect_var_like_declarations(root, source, &mut declarations);
    if declarations.len() != 1 {
        return VarToConstEligibility {
            eligible: false,
            binding: None,
            reason: format!("expected one simple var/let binding, found {}", declarations.len()),
        };
    }
    let declaration = declarations.remove(0);
    if has_reassignment(root, source, &declaration.name, declaration.end_byte) {
        return VarToConstEligibility {
            eligible: false,
            binding: Some(declaration.name),
            reason: "binding is reassigned after declaration".to_string(),
        };
    }
    VarToConstEligibility {
        eligible: true,
        binding: Some(declaration.name),
        reason: "single var/let binding with no later reassignment".to_string(),
    }
}

#[derive(Clone, Debug)]
struct Declaration {
    name: String,
    end_byte: usize,
}

fn collect_var_like_declarations(
    node: tree_sitter::Node<'_>,
    source: &str,
    declarations: &mut Vec<Declaration>,
) {
    if node.kind() == "variable_declarator" {
        if let Some(declaration) = declaration_from_node(node, source) {
            declarations.push(declaration);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_var_like_declarations(child, source, declarations);
    }
}

fn declaration_from_node(node: tree_sitter::Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    if name_node.kind() != "identifier" {
        return None;
    }
    let parent = node.parent()?;
    let parent_text = parent.utf8_text(source.as_bytes()).ok()?.trim_start();
    if !(parent_text.starts_with("let ") || parent_text.starts_with("var ")) {
        return None;
    }
    let name = name_node.utf8_text(source.as_bytes()).ok()?.to_string();
    Some(Declaration {
        name,
        end_byte: node.end_byte(),
    })
}

fn has_reassignment(
    node: tree_sitter::Node<'_>,
    source: &str,
    binding: &str,
    declaration_end: usize,
) -> bool {
    if node.start_byte() >= declaration_end && assignment_to_binding(node, source, binding) {
        return true;
    }
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|child| has_reassignment(child, source, binding, declaration_end));
    found
}

fn assignment_to_binding(node: tree_sitter::Node<'_>, source: &str, binding: &str) -> bool {
    match node.kind() {
        "assignment_expression" | "augmented_assignment_expression" => node
            .child_by_field_name("left")
            .is_some_and(|left| node_text(left, source) == Some(binding)),
        "update_expression" => {
            let mut cursor = node.walk();
            let found = node.children(&mut cursor).any(|child| {
                child.kind() == "identifier" && node_text(child, source) == Some(binding)
            });
            found
        }
        _ => false,
    }
}

fn node_text<'a>(node: tree_sitter::Node<'_>, source: &'a str) -> Option<&'a str> {
    node.utf8_text(source.as_bytes()).ok()
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
    } else if has_any(text, &["remove console", "strip console", "remove debug log", "debug logging"]) {
        Some("remove-debug-logging")
    } else if has_any(text, &["replace literal", "replace string literal", "change literal"]) {
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

fn route(
    intent: &str,
    confidence: f32,
    tools: &[&str],
    budget_tier: &str,
    llm_required: bool,
    edit_candidate: Option<EditCandidate>,
    signals: &[&str],
    reason: &str,
) -> TaskRoute {
    TaskRoute {
        intent: intent.to_string(),
        confidence,
        recommended_tools: tools.iter().map(|tool| (*tool).to_string()).collect(),
        budget_tier: budget_tier.to_string(),
        llm_required,
        edit_candidate,
        signals: signals.iter().map(|signal| (*signal).to_string()).collect(),
        reason: reason.to_string(),
    }
}

fn has_any(text: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| text.contains(term))
}

#[cfg(test)]
mod tests;
