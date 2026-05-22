use super::common::{
    call_args_after, clean_reference, first_string_literal, split_top_level_args, RouteCollector,
};

const GO_METHODS: &[(&str, &str)] = &[
    ("GET", ".GET("),
    ("POST", ".POST("),
    ("PUT", ".PUT("),
    ("PATCH", ".PATCH("),
    ("DELETE", ".DELETE("),
    ("GET", ".Get("),
    ("POST", ".Post("),
    ("PUT", ".Put("),
    ("PATCH", ".Patch("),
    ("DELETE", ".Delete("),
];

pub(super) fn collect(collector: &mut RouteCollector<'_>) {
    for line in collector.lines() {
        let trimmed = line.text.trim();
        if let Some((method, path, handler)) = go_route(trimmed) {
            collector.add_route(
                &method,
                &path,
                handler.as_deref(),
                line.line_no,
                line.start,
                line.end,
            );
        }
    }
}

fn go_route(line: &str) -> Option<(String, String, Option<String>)> {
    if let Some(args) = call_args_after(line, "http.HandleFunc(") {
        let args = split_top_level_args(&args);
        let path = args.first().and_then(|arg| first_string_literal(arg))?;
        let handler = args.get(1).and_then(|arg| clean_reference(arg));
        return Some(("ANY".to_string(), path, handler));
    }
    if let Some(args) = call_args_after(line, ".HandleFunc(") {
        let args = split_top_level_args(&args);
        let path = args.first().and_then(|arg| first_string_literal(arg))?;
        let handler = args.get(1).and_then(|arg| clean_reference(arg));
        let method = line
            .find(".Methods(")
            .and_then(|idx| first_string_literal(&line[idx + ".Methods(".len()..]))
            .unwrap_or_else(|| "ANY".to_string());
        return Some((method.to_ascii_uppercase(), path, handler));
    }
    for (method, marker) in GO_METHODS {
        let Some(args) = call_args_after(line, marker) else {
            continue;
        };
        let args = split_top_level_args(&args);
        let path = args.first().and_then(|arg| first_string_literal(arg))?;
        let handler = args.get(1).and_then(|arg| clean_reference(arg));
        return Some((method.to_string(), path, handler));
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::structure::parse::{extract::routes::extract_route_bindings, Language};

    #[test]
    fn gin_and_gorilla_routes() {
        let source = br#"package main
func routes() {
  r.GET("/users", listUsers)
  router.HandleFunc("/accounts", showAccount).Methods("GET")
}
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let (symbols, edges) = extract_route_bindings(Language::Go, &tree, source);
        assert!(symbols
            .iter()
            .any(|symbol| symbol.display_name == "GET /users"));
        assert!(symbols
            .iter()
            .any(|symbol| symbol.display_name == "GET /accounts"));
        assert!(edges.iter().any(|edge| edge.to_reference == "listUsers"));
        assert!(edges.iter().any(|edge| edge.to_reference == "showAccount"));
    }
}
