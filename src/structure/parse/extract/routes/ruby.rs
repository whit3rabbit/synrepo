use super::common::{first_string_literal, pascal_case, RouteCollector};

const RAILS_METHODS: &[&str] = &["get", "post", "put", "patch", "delete"];

pub(super) fn collect(collector: &mut RouteCollector<'_>) {
    for line in collector.lines() {
        let trimmed = line.text.trim();
        if let Some(resource) = rails_resource(trimmed) {
            let controller = format!("{}Controller", pascal_case(&resource));
            for (method, path, action) in resource_routes(&resource) {
                let target = format!("{controller}::{action}");
                collector.add_route(
                    method,
                    &path,
                    Some(&target),
                    line.line_no,
                    line.start,
                    line.end,
                );
            }
            continue;
        }
        if let Some((method, path, handler)) = rails_route(trimmed) {
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

fn rails_route(line: &str) -> Option<(String, String, Option<String>)> {
    let method = RAILS_METHODS
        .iter()
        .find(|method| line.starts_with(&format!("{method} ")))?;
    let path = first_string_literal(line)?;
    let handler = rails_handler(line);
    Some((method.to_ascii_uppercase(), path, handler))
}

fn rails_resource(line: &str) -> Option<String> {
    let rest = line.strip_prefix("resources ")?;
    if let Some(name) = rest.trim().strip_prefix(':') {
        return Some(
            name.chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect(),
        );
    }
    first_string_literal(rest)
}

fn rails_handler(line: &str) -> Option<String> {
    let handler = first_string_literal(
        line.find("to:")
            .map(|idx| &line[idx + "to:".len()..])
            .or_else(|| line.find("=>").map(|idx| &line[idx + "=>".len()..]))?,
    )?;
    let (controller, action) = handler.split_once('#')?;
    Some(format!("{}Controller::{}", pascal_case(controller), action))
}

fn resource_routes(resource: &str) -> Vec<(&'static str, String, &'static str)> {
    let base = resource.trim_matches('/');
    vec![
        ("GET", format!("/{base}"), "index"),
        ("GET", format!("/{base}/new"), "new"),
        ("POST", format!("/{base}"), "create"),
        ("GET", format!("/{base}/:id"), "show"),
        ("GET", format!("/{base}/:id/edit"), "edit"),
        ("PATCH", format!("/{base}/:id"), "update"),
        ("DELETE", format!("/{base}/:id"), "destroy"),
    ]
}

#[cfg(test)]
mod tests {
    use crate::structure::parse::{extract::routes::extract_route_bindings, Language};

    #[test]
    fn rails_routes_and_resources() {
        let source = br#"Rails.application.routes.draw do
  get "/users", to: "users#index"
  resources :posts
end
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let (symbols, edges) = extract_route_bindings(Language::Ruby, &tree, source);
        assert!(symbols
            .iter()
            .any(|symbol| symbol.display_name == "GET /users"));
        assert!(symbols
            .iter()
            .any(|symbol| symbol.display_name == "GET /posts"));
        assert!(edges
            .iter()
            .any(|edge| edge.to_reference == "UsersController::index"));
        assert!(edges
            .iter()
            .any(|edge| edge.to_reference == "PostsController::index"));
    }
}
