use super::common::{
    first_string_literal, identifier_at, join_paths, method_from_http_name, RouteCollector,
};

struct PendingRoute {
    method: String,
    path: String,
    start: usize,
    end: usize,
}

struct ClassScope {
    name: String,
    prefix: String,
    depth: i32,
}

pub(super) fn collect(collector: &mut RouteCollector<'_>) {
    let mut pending = Vec::<PendingRoute>::new();
    let mut class_scope: Option<ClassScope> = None;

    for line in collector.lines() {
        let trimmed = line.text.trim();
        if let Some((method, path)) = spring_annotation(trimmed) {
            pending.push(PendingRoute {
                method,
                path,
                start: line.start,
                end: line.end,
            });
            continue;
        }
        if let Some(class_name) = class_name(trimmed) {
            let prefix = pending
                .iter()
                .rev()
                .find(|route| route.method == "ANY")
                .map(|route| route.path.clone())
                .unwrap_or_default();
            pending.clear();
            class_scope = Some(ClassScope {
                name: class_name,
                prefix,
                depth: brace_delta(trimmed),
            });
            continue;
        }
        if let Some(scope) = class_scope.as_mut() {
            if let Some(method_name) = method_name(trimmed) {
                for route in pending.drain(..) {
                    let path = join_paths(&scope.prefix, &route.path);
                    let target = format!("{}::{}", scope.name, method_name);
                    collector.add_route(
                        &route.method,
                        &path,
                        Some(&target),
                        line.line_no,
                        route.start,
                        line.end.max(route.end),
                    );
                }
            }
            scope.depth += brace_delta(trimmed);
            if scope.depth <= 0 && trimmed.contains('}') {
                class_scope = None;
            }
        }
    }
}

fn spring_annotation(line: &str) -> Option<(String, String)> {
    let line = line.strip_prefix('@')?;
    let open = line.find('(');
    let name = open.map(|idx| &line[..idx]).unwrap_or(line);
    let method = if name == "RequestMapping" {
        request_mapping_method(line).unwrap_or_else(|| "ANY".to_string())
    } else {
        method_from_http_name(name)?.to_ascii_uppercase()
    };
    let path = open
        .and_then(|idx| first_string_literal(&line[idx + 1..]))
        .unwrap_or_default();
    Some((method, path))
}

fn request_mapping_method(line: &str) -> Option<String> {
    let marker = "RequestMethod.";
    let start = line.find(marker)? + marker.len();
    let method = line[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_alphabetic())
        .collect::<String>();
    (!method.is_empty()).then(|| method.to_ascii_uppercase())
}

fn class_name(line: &str) -> Option<String> {
    let class_pos = line.find("class ")?;
    identifier_at(&line[class_pos + "class ".len()..])
}

fn method_name(line: &str) -> Option<String> {
    if !line.contains('(') || line.starts_with('@') {
        return None;
    }
    if let Some(after_fun) = line.strip_prefix("fun ") {
        return identifier_at(after_fun);
    }
    let before = line.split_once('(')?.0.trim();
    let name = before.split_whitespace().last()?;
    if name == "if" || name == "for" || name == "while" {
        None
    } else {
        identifier_at(name)
    }
}

fn brace_delta(line: &str) -> i32 {
    line.chars().filter(|ch| *ch == '{').count() as i32
        - line.chars().filter(|ch| *ch == '}').count() as i32
}

#[cfg(test)]
mod tests {
    use crate::structure::parse::{extract::routes::extract_route_bindings, Language};

    #[test]
    fn spring_routes_include_class_prefix() {
        let source = br#"@RequestMapping("/api")
class UsersController {
  @GetMapping("/users")
  public List<User> listUsers() { return List.of(); }
}
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let (symbols, edges) = extract_route_bindings(Language::Java, &tree, source);
        assert!(symbols
            .iter()
            .any(|symbol| symbol.display_name == "GET /api/users"));
        assert!(edges
            .iter()
            .any(|edge| edge.to_reference == "UsersController::listUsers"));
    }
}
