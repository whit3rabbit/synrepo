use super::common::{first_string_literal, identifier_at, join_paths, RouteCollector};

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
        for (method, path) in csharp_attributes(trimmed) {
            pending.push(PendingRoute {
                method,
                path,
                start: line.start,
                end: line.end,
            });
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
                for route in pending.drain(..).filter(|route| route.method != "ANY") {
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

fn csharp_attributes(line: &str) -> Vec<(String, String)> {
    let mut routes = Vec::new();
    for part in line.split('[').skip(1) {
        let Some(attr) = part.split_once(']').map(|(attr, _)| attr.trim()) else {
            continue;
        };
        let name = attr.split(['(', ' ']).next().unwrap_or(attr);
        let path = first_string_literal(attr).unwrap_or_default();
        let method = match name {
            "Route" => "ANY",
            "HttpGet" => "GET",
            "HttpPost" => "POST",
            "HttpPut" => "PUT",
            "HttpPatch" => "PATCH",
            "HttpDelete" => "DELETE",
            "HttpHead" => "HEAD",
            _ => continue,
        };
        routes.push((method.to_string(), path));
    }
    routes
}

fn class_name(line: &str) -> Option<String> {
    let class_pos = line.find("class ")?;
    identifier_at(&line[class_pos + "class ".len()..])
}

fn method_name(line: &str) -> Option<String> {
    if !line.contains('(') || line.starts_with('[') {
        return None;
    }
    let before = line.split_once('(')?.0.trim();
    let name = before.split_whitespace().last()?;
    identifier_at(name)
}

fn brace_delta(line: &str) -> i32 {
    line.chars().filter(|ch| *ch == '{').count() as i32
        - line.chars().filter(|ch| *ch == '}').count() as i32
}

#[cfg(test)]
mod tests {
    use crate::structure::parse::{extract::routes::extract_route_bindings, Language};

    #[test]
    fn aspnet_routes_include_controller_prefix() {
        let source = br#"[Route("api/users")]
public class UsersController {
  [HttpGet("{id}")]
  public IActionResult Show() { return Ok(); }
}
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let (symbols, edges) = extract_route_bindings(Language::CSharp, &tree, source);
        assert!(symbols
            .iter()
            .any(|symbol| symbol.display_name == "GET /api/users/{id}"));
        assert!(edges
            .iter()
            .any(|edge| edge.to_reference == "UsersController::Show"));
    }
}
