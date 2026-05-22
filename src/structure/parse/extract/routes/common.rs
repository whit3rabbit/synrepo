use crate::structure::{
    graph::{EdgeKind, SymbolKind, Visibility},
    parse::{ExtractedEdge, ExtractedSymbol},
};

const ROUTE_LIMIT: usize = 512;
pub(super) const HTTP_METHODS: &[&str] =
    &["get", "post", "put", "patch", "delete", "head", "options"];

pub(super) struct RouteCollector<'a> {
    content: &'a [u8],
    symbols: Vec<ExtractedSymbol>,
    edges: Vec<ExtractedEdge>,
}

impl<'a> RouteCollector<'a> {
    pub(super) fn new(content: &'a [u8]) -> Self {
        Self {
            content,
            symbols: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub(super) fn lines(&self) -> Vec<LineSpan<'a>> {
        line_spans(self.content)
    }

    pub(super) fn add_route(
        &mut self,
        method: &str,
        path: &str,
        handler: Option<&str>,
        line_no: usize,
        start: usize,
        end: usize,
    ) {
        if self.symbols.len() >= ROUTE_LIMIT {
            return;
        }
        let method = normalize_method(method);
        let path = normalize_path(path);
        let display_name = format!("{method} {path}");
        let qualified_name = format!("route::{display_name} @{line_no}");
        let range = (start as u32, end as u32);
        let body = self.content.get(start..end).unwrap_or_default();
        self.symbols.push(ExtractedSymbol {
            qualified_name: qualified_name.clone(),
            display_name,
            kind: SymbolKind::Route,
            visibility: Visibility::Public,
            body_byte_range: range,
            body_hash: hex::encode(blake3::hash(body).as_bytes()),
            signature: handler.map(|name| format!("{method} {path} -> {name}")),
            doc_comment: None,
        });
        if let Some(handler) = handler.filter(|name| !name.is_empty()) {
            self.edges.push(ExtractedEdge {
                from_qualified_name: qualified_name,
                to_reference: handler.to_string(),
                kind: EdgeKind::References,
            });
        }
    }

    pub(super) fn finish(self) -> (Vec<ExtractedSymbol>, Vec<ExtractedEdge>) {
        (self.symbols, self.edges)
    }
}

#[derive(Clone, Copy)]
pub(super) struct LineSpan<'a> {
    pub(super) text: &'a str,
    pub(super) line_no: usize,
    pub(super) start: usize,
    pub(super) end: usize,
}

pub(super) fn line_spans(content: &[u8]) -> Vec<LineSpan<'_>> {
    let source = std::str::from_utf8(content).unwrap_or("");
    let mut spans = Vec::new();
    let mut start = 0usize;
    for (idx, line) in source.split_inclusive('\n').enumerate() {
        let end = start + line.len();
        spans.push(LineSpan {
            text: line.trim_end_matches('\n'),
            line_no: idx + 1,
            start,
            end,
        });
        start = end;
    }
    if !source.is_empty() && !source.ends_with('\n') && spans.is_empty() {
        spans.push(LineSpan {
            text: source,
            line_no: 1,
            start: 0,
            end: source.len(),
        });
    }
    spans
}

pub(super) fn normalize_method(method: &str) -> String {
    let method = method.trim();
    if method.is_empty() {
        "ANY".to_string()
    } else {
        method.to_ascii_uppercase()
    }
}

pub(super) fn normalize_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() || path == "/" {
        return "/".to_string();
    }
    let path = path.trim_matches('/');
    if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{path}")
    }
}

pub(super) fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = normalize_path(prefix);
    let path = normalize_path(path);
    if prefix == "/" {
        return path;
    }
    if path == "/" {
        return prefix;
    }
    format!(
        "{}/{}",
        prefix.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

pub(super) fn first_string_literal(input: &str) -> Option<String> {
    all_string_literals(input).into_iter().next()
}

pub(super) fn all_string_literals(input: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch != '"' && ch != '\'' && ch != '`' {
            continue;
        }
        let quote = ch;
        let rest = &input[idx + ch.len_utf8()..];
        let mut escaped = false;
        for (end, c) in rest.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c == quote {
                literals.push(rest[..end].to_string());
                while chars
                    .peek()
                    .is_some_and(|(next_idx, _)| *next_idx <= idx + ch.len_utf8() + end)
                {
                    chars.next();
                }
                break;
            }
        }
    }
    literals
}

pub(super) fn call_args_after(line: &str, marker: &str) -> Option<String> {
    let start = line.find(marker)? + marker.len();
    let after_open = &line[start..];
    Some(balanced_args(after_open))
}

fn balanced_args(input: &str) -> String {
    let mut depth = 1i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (idx, ch) in input.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        if matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
            continue;
        }
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return input[..idx].to_string();
                }
            }
            _ => {}
        }
    }
    input.trim_end_matches([';', ')']).to_string()
}

pub(super) fn split_top_level_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (idx, ch) in input.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        if matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
            continue;
        }
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(input[start..idx].trim().to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = input[start..].trim();
    if !tail.is_empty() {
        args.push(tail.to_string());
    }
    args
}

pub(super) fn identifier_at(input: &str) -> Option<String> {
    let trimmed = input.trim_start();
    let mut out = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '.') {
            out.push(ch);
        } else {
            break;
        }
    }
    clean_reference(&out)
}

pub(super) fn clean_reference(input: &str) -> Option<String> {
    let input = input.trim().trim_start_matches('&');
    let input = input.trim_end_matches("::class");
    let input = input.trim_matches(['"', '\'', '`', ' ', '[', ']', '\\']);
    if input.is_empty() || input.contains("=>") || input.contains("->") {
        return None;
    }
    let normalized = input.replace('.', "::");
    (!normalized.is_empty()).then_some(normalized)
}

pub(super) fn last_identifier(input: &str) -> Option<String> {
    clean_reference(input)?
        .rsplit("::")
        .find(|part| !part.is_empty())
        .map(ToString::to_string)
}

pub(super) fn method_from_http_name(name: &str) -> Option<&'static str> {
    let name = name.trim().trim_start_matches('@').trim_start_matches('[');
    let lower = name.to_ascii_lowercase();
    HTTP_METHODS
        .iter()
        .find(|method| lower == **method || lower.ends_with(&format!("{}mapping", method)))
        .copied()
}

pub(super) fn pascal_case(input: &str) -> String {
    input
        .split(['_', '-', '/', ':'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{RouteCollector, ROUTE_LIMIT};

    #[test]
    fn route_collector_caps_routes_per_file() {
        let source = b"route";
        let mut collector = RouteCollector::new(source);
        for idx in 0..(ROUTE_LIMIT + 10) {
            collector.add_route("GET", &format!("/r{idx}"), None, idx + 1, 0, source.len());
        }
        let (symbols, edges) = collector.finish();
        assert_eq!(symbols.len(), ROUTE_LIMIT);
        assert!(edges.is_empty());
    }
}
