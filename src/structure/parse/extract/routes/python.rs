use super::common::{
    all_string_literals, call_args_after, first_string_literal, last_identifier,
    split_top_level_args, RouteCollector, HTTP_METHODS,
};

struct PendingRoute {
    method: String,
    path: String,
    start: usize,
    end: usize,
}

pub(super) fn collect(collector: &mut RouteCollector<'_>) {
    let mut pending = Vec::<PendingRoute>::new();
    for line in collector.lines() {
        let trimmed = line.text.trim();
        if let Some(routes) = decorator_routes(trimmed, line.start, line.end) {
            pending.extend(routes);
            continue;
        }
        if let Some((path, handler)) = django_route_call(trimmed) {
            collector.add_route(
                "ANY",
                &path,
                handler.as_deref(),
                line.line_no,
                line.start,
                line.end,
            );
        }
        if pending.is_empty() {
            continue;
        }
        if let Some(handler) = python_def_name(trimmed).or_else(|| python_class_name(trimmed)) {
            for route in pending.drain(..) {
                collector.add_route(
                    &route.method,
                    &route.path,
                    Some(&handler),
                    line.line_no,
                    route.start,
                    line.end.max(route.end),
                );
            }
        }
    }
}

fn decorator_routes(line: &str, start: usize, end: usize) -> Option<Vec<PendingRoute>> {
    let line = line.strip_prefix('@')?;
    let open = line.find('(')?;
    let name = &line[..open];
    let method = name.rsplit('.').next()?.to_ascii_lowercase();
    let args = &line[open + 1..];
    let path = first_string_literal(args)?;
    if HTTP_METHODS.contains(&method.as_str()) {
        return Some(vec![PendingRoute {
            method: method.to_ascii_uppercase(),
            path,
            start,
            end,
        }]);
    }
    if method != "route" {
        return None;
    }
    let methods = flask_methods(args);
    let methods = if methods.is_empty() {
        vec!["ANY".to_string()]
    } else {
        methods
    };
    Some(
        methods
            .into_iter()
            .map(|method| PendingRoute {
                method,
                path: path.clone(),
                start,
                end,
            })
            .collect(),
    )
}

fn flask_methods(args: &str) -> Vec<String> {
    let Some(methods_pos) = args.find("methods") else {
        return Vec::new();
    };
    all_string_literals(&args[methods_pos..])
        .into_iter()
        .map(|method| method.to_ascii_uppercase())
        .filter(|method| method != "METHODS")
        .collect()
}

fn django_route_call(line: &str) -> Option<(String, Option<String>)> {
    let marker = ["path(", "re_path(", "url("]
        .into_iter()
        .find(|marker| line.contains(marker))?;
    let args = call_args_after(line, marker)?;
    let args = split_top_level_args(&args);
    let path = args.first().and_then(|arg| first_string_literal(arg))?;
    let view = args.get(1)?;
    if view.contains("include(") {
        return Some((path, None));
    }
    let handler = if let Some(as_view) = view.find(".as_view") {
        last_identifier(&view[..as_view])
    } else {
        last_identifier(view)
    };
    Some((path, handler))
}

fn python_def_name(line: &str) -> Option<String> {
    let after = line
        .strip_prefix("async def ")
        .or_else(|| line.strip_prefix("def "))?;
    let name = after
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    (!name.is_empty()).then_some(name)
}

fn python_class_name(line: &str) -> Option<String> {
    let after = line.strip_prefix("class ")?;
    let name = after
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use crate::structure::parse::{extract::routes::extract_route_bindings, Language};

    fn parse(source: &[u8]) -> (Vec<String>, Vec<String>) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let (symbols, edges) = extract_route_bindings(Language::Python, &tree, source);
        (
            symbols
                .into_iter()
                .map(|symbol| symbol.display_name)
                .collect(),
            edges.into_iter().map(|edge| edge.to_reference).collect(),
        )
    }

    #[test]
    fn fastapi_flask_and_django_routes() {
        let (symbols, edges) = parse(
            br#"@router.get("/users")
def list_users(): pass
@app.route("/login", methods=["POST"])
def login(): pass
urlpatterns = [path("accounts/", AccountView.as_view())]
"#,
        );
        assert!(symbols.iter().any(|name| name == "GET /users"));
        assert!(symbols.iter().any(|name| name == "POST /login"));
        assert!(symbols.iter().any(|name| name == "ANY /accounts"));
        assert!(edges.iter().any(|name| name == "list_users"));
        assert!(edges.iter().any(|name| name == "login"));
        assert!(edges.iter().any(|name| name == "AccountView"));
    }
}
