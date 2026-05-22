use super::common::{
    first_string_literal, identifier_at, method_from_http_name, RouteCollector, HTTP_METHODS,
};

pub(super) fn collect(collector: &mut RouteCollector<'_>) {
    let mut pending_rocket: Option<(String, String, usize, usize)> = None;

    for line in collector.lines() {
        let trimmed = line.text.trim();
        if let Some((method, path)) = rocket_route_attr(trimmed) {
            pending_rocket = Some((method, path, line.start, line.end));
            continue;
        }
        if let Some((method, path, handler)) = rust_route_chain(trimmed) {
            collector.add_route(
                &method,
                &path,
                handler.as_deref(),
                line.line_no,
                line.start,
                line.end,
            );
        }
        if let Some((method, path, start, _)) = pending_rocket.take() {
            if let Some(handler) = rust_fn_name(trimmed) {
                collector.add_route(
                    &method,
                    &path,
                    Some(&handler),
                    line.line_no,
                    start,
                    line.end,
                );
            } else {
                pending_rocket = Some((method, path, start, line.end));
            }
        }
    }
}

fn rocket_route_attr(line: &str) -> Option<(String, String)> {
    let route = line.strip_prefix("#[")?.strip_suffix(']')?;
    let open = route.find('(')?;
    let method = &route[..open];
    method_from_http_name(method)?;
    let path = first_string_literal(&route[open + 1..])?;
    Some((method.to_ascii_uppercase(), path))
}

fn rust_route_chain(line: &str) -> Option<(String, String, Option<String>)> {
    let route_pos = line.find(".route(")?;
    let after_route = &line[route_pos + ".route(".len()..];
    let path = first_string_literal(after_route)?;

    for method in HTTP_METHODS {
        if let Some(handler) = handler_inside_call(after_route, method) {
            return Some((method.to_ascii_uppercase(), path, Some(handler)));
        }
    }
    if let Some((method, handler)) = handler_after_method_to(after_route) {
        return Some((method, path, Some(handler)));
    }
    if let Some(handler) = handler_after_to(after_route) {
        return Some(("ANY".to_string(), path, Some(handler)));
    }
    Some(("ANY".to_string(), path, None))
}

fn handler_inside_call(input: &str, call: &str) -> Option<String> {
    let needle = format!("{call}(");
    let start = input.find(&needle)? + needle.len();
    identifier_at(&input[start..])
}

fn handler_after_to(input: &str) -> Option<String> {
    let start = input.find(".to(")? + ".to(".len();
    identifier_at(&input[start..])
}

fn handler_after_method_to(input: &str) -> Option<(String, String)> {
    for method in HTTP_METHODS {
        let needle = format!("{method}().to(");
        if let Some(start) = input.find(&needle).map(|idx| idx + needle.len()) {
            let handler = identifier_at(&input[start..])?;
            return Some((method.to_ascii_uppercase(), handler));
        }
    }
    None
}

fn rust_fn_name(line: &str) -> Option<String> {
    let after = line
        .strip_prefix("pub fn ")
        .or_else(|| line.strip_prefix("async fn "))
        .or_else(|| line.strip_prefix("pub async fn "))
        .or_else(|| line.strip_prefix("fn "))?;
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
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let (symbols, edges) = extract_route_bindings(Language::Rust, &tree, source);
        (
            symbols
                .into_iter()
                .map(|symbol| symbol.display_name)
                .collect(),
            edges.into_iter().map(|edge| edge.to_reference).collect(),
        )
    }

    #[test]
    fn axum_and_actix_routes_reference_handlers() {
        let (symbols, edges) = parse(
            br#"fn app() {
    Router::new().route("/users", get(list_users));
    cfg.route("/users", web::post().to(create_user));
}
"#,
        );
        assert!(symbols.iter().any(|name| name == "GET /users"));
        assert!(symbols.iter().any(|name| name == "POST /users"));
        assert!(edges.iter().any(|name| name == "list_users"));
        assert!(edges.iter().any(|name| name == "create_user"));
    }
}
