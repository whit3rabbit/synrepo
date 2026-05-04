use super::VarToConstEligibility;

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
            reason: format!(
                "expected one simple var/let binding, found {}",
                declarations.len()
            ),
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
