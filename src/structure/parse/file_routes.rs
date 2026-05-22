use crate::structure::{
    graph::{EdgeKind, SymbolKind, Visibility},
    parse::{ExtractedEdge, ExtractedSymbol},
};

const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// Extract route symbols from framework file-route conventions.
pub fn extract_file_routes(
    relative_path: &str,
    content: &[u8],
) -> (Vec<ExtractedSymbol>, Vec<ExtractedEdge>) {
    let mut routes = Vec::new();
    let mut edges = Vec::new();
    if let Some(path) = react_router_fs_path(relative_path) {
        let handler = default_export_name(content);
        push_file_route(
            &mut routes,
            &mut edges,
            "react_router",
            "ANY",
            &path,
            handler.as_deref(),
            content,
        );
    }
    if let Some(kind) = sveltekit_file_route(relative_path) {
        match kind {
            SvelteKitRoute::Page(path) => push_file_route(
                &mut routes,
                &mut edges,
                "sveltekit",
                "ANY",
                &path,
                None,
                content,
            ),
            SvelteKitRoute::Server(path) => {
                for method in sveltekit_server_methods(content) {
                    push_file_route(
                        &mut routes,
                        &mut edges,
                        "sveltekit",
                        method,
                        &path,
                        (method != "ANY").then_some(method),
                        content,
                    );
                }
            }
        }
    }
    (routes, edges)
}

fn push_file_route(
    routes: &mut Vec<ExtractedSymbol>,
    edges: &mut Vec<ExtractedEdge>,
    framework: &str,
    method: &str,
    path: &str,
    handler: Option<&str>,
    content: &[u8],
) {
    let path = normalize_path(path);
    let display_name = format!("{method} {path}");
    let qualified_name = format!("route::{display_name} @file:{framework}");
    routes.push(ExtractedSymbol {
        qualified_name: qualified_name.clone(),
        display_name,
        kind: SymbolKind::Route,
        visibility: Visibility::Public,
        body_byte_range: (0, content.len() as u32),
        body_hash: hex::encode(blake3::hash(content).as_bytes()),
        signature: handler.map(|name| format!("{method} {path} -> {name}")),
        doc_comment: None,
    });
    if let Some(handler) = handler.filter(|name| !name.is_empty()) {
        edges.push(ExtractedEdge {
            from_qualified_name: qualified_name,
            to_reference: handler.to_string(),
            kind: EdgeKind::References,
        });
    }
}

fn react_router_fs_path(relative_path: &str) -> Option<String> {
    let rest = route_relative_segments(relative_path, &["app", "routes"])?;
    let route_id = react_router_route_id(&rest)?;
    react_router_route_path(&route_id)
}

fn react_router_route_id(segments: &[&str]) -> Option<String> {
    let file_name = *segments.last()?;
    let stem = route_module_stem(file_name)?;
    if stem == "route" {
        if segments.len() < 2 {
            return None;
        }
        return Some(segments[..segments.len() - 1].join("."));
    }
    (segments.len() == 1).then_some(stem.to_string())
}

fn route_module_stem(file_name: &str) -> Option<&str> {
    for suffix in [".tsx", ".jsx", ".ts", ".js"] {
        if let Some(stem) = file_name.strip_suffix(suffix) {
            return Some(stem);
        }
    }
    None
}

fn react_router_route_path(route_id: &str) -> Option<String> {
    let mut path_segments = Vec::new();
    let mut saw_index = false;
    for raw in route_id.split('.') {
        if raw == "_index" {
            saw_index = true;
            continue;
        }
        let Some(segment) = react_router_segment(raw) else {
            continue;
        };
        path_segments.push(segment);
    }
    if path_segments.is_empty() && !saw_index {
        return None;
    }
    Some(path_from_segments(&path_segments))
}

fn react_router_segment(raw: &str) -> Option<String> {
    if raw.starts_with('_') {
        return None;
    }
    let raw = raw.strip_suffix('_').unwrap_or(raw);
    let (raw, optional) = match optional_segment(raw) {
        Some(inner) => (inner, true),
        None => (raw, false),
    };
    let segment = if let Some(param) = raw.strip_prefix('$') {
        if param.is_empty() {
            "*".to_string()
        } else if optional {
            format!(":{}?", unescape_react_router(param))
        } else {
            format!(":{}", unescape_react_router(param))
        }
    } else if optional {
        format!("{}?", unescape_react_router(raw))
    } else {
        unescape_react_router(raw)
    };
    (!segment.is_empty()).then_some(segment)
}

fn optional_segment(raw: &str) -> Option<&str> {
    raw.strip_prefix('(')?.strip_suffix(')')
}

fn unescape_react_router(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '[' {
            out.push(ch);
            continue;
        }
        let mut escaped = String::new();
        for next in chars.by_ref() {
            if next == ']' {
                break;
            }
            escaped.push(next);
        }
        out.push_str(&escaped);
    }
    out
}

enum SvelteKitRoute {
    Page(String),
    Server(String),
}

fn sveltekit_file_route(relative_path: &str) -> Option<SvelteKitRoute> {
    let rest = route_relative_segments(relative_path, &["src", "routes"])?;
    let file_name = *rest.last()?;
    let path = sveltekit_route_path(&rest[..rest.len().saturating_sub(1)])?;
    match file_name {
        "+page.svelte" => Some(SvelteKitRoute::Page(path)),
        "+server.js" | "+server.ts" => Some(SvelteKitRoute::Server(path)),
        _ => None,
    }
}

fn sveltekit_route_path(segments: &[&str]) -> Option<String> {
    let mut path_segments = Vec::new();
    for raw in segments {
        if raw.starts_with('(') && raw.ends_with(')') {
            continue;
        }
        path_segments.push(sveltekit_segment(raw)?);
    }
    Some(path_from_segments(&path_segments))
}

fn sveltekit_segment(raw: &str) -> Option<String> {
    if let Some(param) = raw.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
        return (!param.is_empty()).then_some(format!(":{param}?"));
    }
    if let Some(param) = raw.strip_prefix("[...").and_then(|s| s.strip_suffix(']')) {
        return (!param.is_empty()).then_some(format!("*{param}"));
    }
    if let Some(param) = raw.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return (!param.is_empty()).then_some(format!(":{param}"));
    }
    (!raw.is_empty()).then_some(raw.to_string())
}

fn route_relative_segments<'a>(relative_path: &'a str, marker: &[&str]) -> Option<Vec<&'a str>> {
    let segments: Vec<_> = relative_path.split('/').collect();
    let start = segments
        .windows(marker.len())
        .position(|window| window == marker)?
        + marker.len();
    (start < segments.len()).then(|| segments[start..].to_vec())
}

fn path_from_segments(segments: &[String]) -> String {
    if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    }
}

fn normalize_path(path: &str) -> String {
    if path == "/" {
        return "/".to_string();
    }
    format!("/{}", path.trim_matches('/'))
}

fn sveltekit_server_methods(content: &[u8]) -> Vec<&'static str> {
    let source = std::str::from_utf8(content).unwrap_or("");
    let methods: Vec<_> = HTTP_METHODS
        .iter()
        .copied()
        .filter(|method| exports_method(source, method))
        .collect();
    if methods.is_empty() {
        vec!["ANY"]
    } else {
        methods
    }
}

fn exports_method(source: &str, method: &str) -> bool {
    source.lines().any(|line| {
        let line = line.trim_start();
        [
            format!("export function {method}"),
            format!("export async function {method}"),
            format!("export const {method}"),
            format!("export let {method}"),
            format!("export var {method}"),
        ]
        .iter()
        .any(|prefix| has_token_prefix(line, prefix))
    })
}

fn has_token_prefix(line: &str, prefix: &str) -> bool {
    let Some(rest) = line.strip_prefix(prefix) else {
        return false;
    };
    rest.chars()
        .next()
        .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
}

fn default_export_name(content: &[u8]) -> Option<String> {
    let source = std::str::from_utf8(content).ok()?;
    source.lines().find_map(|line| {
        let line = line.trim_start();
        for prefix in ["export default function ", "export default class "] {
            if let Some(rest) = line.strip_prefix(prefix) {
                return identifier_at(rest);
            }
        }
        None
    })
}

fn identifier_at(input: &str) -> Option<String> {
    let mut out = String::new();
    for ch in input.trim_start().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            break;
        }
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::extract_file_routes;

    fn names(path: &str, source: &[u8]) -> Vec<String> {
        extract_file_routes(path, source)
            .0
            .into_iter()
            .map(|symbol| symbol.display_name)
            .collect()
    }

    #[test]
    fn extracts_sveltekit_page_and_server_file_routes() {
        assert_eq!(
            names("src/routes/(app)/blog/[slug]/+page.svelte", b"<h1 />"),
            vec!["ANY /blog/:slug"]
        );
        assert_eq!(
            names(
                "src/routes/api/[...rest]/+server.ts",
                b"export function GET() {}\nexport async function POST() {}\n",
            ),
            vec!["GET /api/*rest", "POST /api/*rest"]
        );
    }

    #[test]
    fn extracts_react_router_file_routes() {
        assert_eq!(
            names(
                "app/routes/concerts.$city.tsx",
                b"export default function ConcertCity() {}",
            ),
            vec!["ANY /concerts/:city"]
        );
        assert_eq!(
            names("app/routes/_auth.login/route.tsx", b""),
            vec!["ANY /login"]
        );
        assert_eq!(names("app/routes/_auth.tsx", b""), Vec::<String>::new());
    }
}
