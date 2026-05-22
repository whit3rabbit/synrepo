use super::common::{
    call_args_after, clean_reference, first_string_literal, split_top_level_args, RouteCollector,
};

const LARAVEL_METHODS: &[(&str, &str)] = &[
    ("GET", "Route::get("),
    ("POST", "Route::post("),
    ("PUT", "Route::put("),
    ("PATCH", "Route::patch("),
    ("DELETE", "Route::delete("),
    ("ANY", "Route::any("),
];

pub(super) fn collect(collector: &mut RouteCollector<'_>) {
    for line in collector.lines() {
        let trimmed = line.text.trim();
        if let Some((path, controller)) = laravel_resource(trimmed) {
            for (method, suffix, action) in resource_routes(&path) {
                let target = controller
                    .as_ref()
                    .map(|controller| format!("{controller}::{action}"));
                collector.add_route(
                    method,
                    &suffix,
                    target.as_deref(),
                    line.line_no,
                    line.start,
                    line.end,
                );
            }
            continue;
        }
        if let Some((method, path, handler)) = laravel_route(trimmed) {
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

fn laravel_route(line: &str) -> Option<(String, String, Option<String>)> {
    for (method, marker) in LARAVEL_METHODS {
        let Some(args) = call_args_after(line, marker) else {
            continue;
        };
        let args = split_top_level_args(&args);
        let path = args.first().and_then(|arg| first_string_literal(arg))?;
        let handler = args.get(1).and_then(|arg| laravel_handler(arg));
        return Some((method.to_string(), path, handler));
    }
    None
}

fn laravel_resource(line: &str) -> Option<(String, Option<String>)> {
    let args = call_args_after(line, "Route::resource(")?;
    let args = split_top_level_args(&args);
    let path = args.first().and_then(|arg| first_string_literal(arg))?;
    let controller = args.get(1).and_then(|arg| controller_name(arg));
    Some((path, controller))
}

fn laravel_handler(arg: &str) -> Option<String> {
    if let Some(lit) = first_string_literal(arg) {
        if let Some((controller, action)) = lit.split_once('@') {
            return Some(format!("{controller}::{action}"));
        }
    }
    let controller = controller_name(arg)?;
    let action = super::common::all_string_literals(arg)
        .into_iter()
        .find(|literal| !literal.contains('\\') && !literal.contains('@'))?;
    Some(format!("{controller}::{action}"))
}

fn controller_name(arg: &str) -> Option<String> {
    if let Some(class_pos) = arg.find("::class") {
        return clean_reference(&arg[..class_pos]);
    }
    first_string_literal(arg)
        .and_then(|literal| literal.split('@').next().and_then(clean_reference))
}

fn resource_routes(path: &str) -> Vec<(&'static str, String, &'static str)> {
    let base = path.trim_matches('/');
    vec![
        ("GET", format!("/{base}"), "index"),
        ("GET", format!("/{base}/create"), "create"),
        ("POST", format!("/{base}"), "store"),
        ("GET", format!("/{base}/{{id}}"), "show"),
        ("GET", format!("/{base}/{{id}}/edit"), "edit"),
        ("PUT", format!("/{base}/{{id}}"), "update"),
        ("PATCH", format!("/{base}/{{id}}"), "update"),
        ("DELETE", format!("/{base}/{{id}}"), "destroy"),
    ]
}

#[cfg(test)]
mod tests {
    use crate::structure::parse::{extract::routes::extract_route_bindings, Language};

    #[test]
    fn laravel_controller_and_resource_routes() {
        let source = br#"<?php
Route::get('/users', [UserController::class, 'index']);
Route::resource('posts', PostController::class);
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let (symbols, edges) = extract_route_bindings(Language::Php, &tree, source);
        assert!(symbols
            .iter()
            .any(|symbol| symbol.display_name == "GET /users"));
        assert!(symbols
            .iter()
            .any(|symbol| symbol.display_name == "GET /posts"));
        assert!(edges
            .iter()
            .any(|edge| edge.to_reference == "UserController::index"));
        assert!(edges
            .iter()
            .any(|edge| edge.to_reference == "PostController::index"));
    }
}
