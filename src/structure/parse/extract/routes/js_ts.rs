use super::common::{
    call_args_after, clean_reference, first_string_literal, identifier_at, join_paths,
    method_from_http_name, split_top_level_args, RouteCollector, HTTP_METHODS,
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
    let mut pending_controller: Option<String> = None;
    let mut pending_routes = Vec::<PendingRoute>::new();
    let mut class_scope: Option<ClassScope> = None;

    for line in collector.lines() {
        let trimmed = line.text.trim();
        if let Some((method, path, handler)) = express_route(trimmed) {
            collector.add_route(
                &method,
                &path,
                handler.as_deref(),
                line.line_no,
                line.start,
                line.end,
            );
        }
        if let Some(prefix) = controller_decorator(trimmed) {
            pending_controller = Some(prefix);
            continue;
        }
        if let Some((method, path)) = nest_route_decorator(trimmed) {
            pending_routes.push(PendingRoute {
                method,
                path,
                start: line.start,
                end: line.end,
            });
            continue;
        }
        if let Some(class_name) = class_name(trimmed) {
            class_scope = Some(ClassScope {
                name: class_name,
                prefix: pending_controller.take().unwrap_or_default(),
                depth: brace_delta(trimmed),
            });
            continue;
        }
        if let Some(scope) = class_scope.as_mut() {
            if let Some(method_name) = method_name(trimmed) {
                for route in pending_routes.drain(..) {
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

fn express_route(line: &str) -> Option<(String, String, Option<String>)> {
    for method in HTTP_METHODS.iter().chain(["use"].iter()) {
        let marker = format!(".{method}(");
        let Some(args) = call_args_after(line, &marker) else {
            continue;
        };
        let args = split_top_level_args(&args);
        let path = args.first().and_then(|arg| first_string_literal(arg))?;
        let handler = args
            .iter()
            .skip(1)
            .rev()
            .find_map(|arg| clean_reference(arg).or_else(|| identifier_at(arg)));
        let method = if *method == "use" {
            "ANY".to_string()
        } else {
            method.to_ascii_uppercase()
        };
        return Some((method, path, handler));
    }
    None
}

fn controller_decorator(line: &str) -> Option<String> {
    line.strip_prefix("@Controller")
        .and_then(|_| first_string_literal(line))
        .or_else(|| line.strip_prefix("@Controller").map(|_| String::new()))
}

fn nest_route_decorator(line: &str) -> Option<(String, String)> {
    let line = line.strip_prefix('@')?;
    let open = line.find('(')?;
    let method = method_from_http_name(&line[..open])?;
    let path = first_string_literal(&line[open + 1..]).unwrap_or_default();
    Some((method.to_ascii_uppercase(), path))
}

fn class_name(line: &str) -> Option<String> {
    let class_pos = line.find("class ")?;
    let after = &line[class_pos + "class ".len()..];
    identifier_at(after)
}

fn method_name(line: &str) -> Option<String> {
    if line.starts_with("constructor") || !line.contains('(') || line.starts_with('@') {
        return None;
    }
    let before = line.split_once('(')?.0.trim();
    let name = before.split_whitespace().last().and_then(clean_reference)?;
    (!["if", "for", "while", "switch"].contains(&name.as_str())).then_some(name)
}

fn brace_delta(line: &str) -> i32 {
    line.chars().filter(|ch| *ch == '{').count() as i32
        - line.chars().filter(|ch| *ch == '}').count() as i32
}

#[cfg(test)]
mod tests {
    use crate::structure::parse::{extract::routes::extract_route_bindings, Language};

    fn parse(source: &[u8]) -> (Vec<String>, Vec<String>) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let (symbols, edges) = extract_route_bindings(Language::TypeScript, &tree, source);
        (
            symbols
                .into_iter()
                .map(|symbol| symbol.display_name)
                .collect(),
            edges.into_iter().map(|edge| edge.to_reference).collect(),
        )
    }

    #[test]
    fn express_middleware_and_nest_controller_routes() {
        let (symbols, edges) = parse(
            br#"router.post("/users", auth, createUser);
@Controller("/users")
class UsersController {
  @Get(":id")
  show() {}
}
"#,
        );
        assert!(symbols.iter().any(|name| name == "POST /users"));
        assert!(symbols.iter().any(|name| name == "GET /users/:id"));
        assert!(edges.iter().any(|name| name == "createUser"));
        assert!(edges.iter().any(|name| name == "UsersController::show"));
    }
}
