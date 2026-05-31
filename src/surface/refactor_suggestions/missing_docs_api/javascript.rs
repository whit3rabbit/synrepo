use std::collections::BTreeSet;

use super::scan::{
    byte_at, consume_keyword, find_matching, insert_identifier, is_assignment_at, is_identifier,
    keyword_at, next_decl_separator, previous_non_ws, read_ident, read_string_literal, skip_ws,
    split_top_level_commas, strip_js_comments, top_level_colon,
};

pub(super) fn exported_names(source: &str) -> BTreeSet<String> {
    let source = strip_js_comments(source);
    let mut names = BTreeSet::new();
    collect_esm_exports(&source, &mut names);
    collect_commonjs_exports(&source, &mut names);
    names
}

fn collect_esm_exports(source: &str, names: &mut BTreeSet<String>) {
    let mut idx = 0;
    while idx < source.len() {
        if !keyword_at(source, idx, "export") {
            idx += 1;
            continue;
        }
        let mut cursor = skip_ws(source, idx + "export".len());
        if let Some(next) = consume_keyword(source, cursor, "default") {
            cursor = skip_ws(source, next);
            collect_default_export(source, cursor, names);
        } else if let Some(next) = consume_keyword(source, cursor, "async") {
            cursor = skip_ws(source, next);
            if let Some(after_function) = consume_keyword(source, cursor, "function") {
                insert_function_name(source, after_function, names);
            }
        } else if let Some(after_function) = consume_keyword(source, cursor, "function") {
            insert_function_name(source, after_function, names);
        } else if let Some(after_class) = consume_keyword(source, cursor, "class") {
            insert_identifier(source, after_class, names);
        } else if let Some(after_decl) = consume_variable_keyword(source, cursor) {
            collect_variable_declarations(source, after_decl, names);
        } else if byte_at(source, cursor) == Some(b'{') {
            collect_export_specifiers(source, cursor + 1, names);
        }
        idx += "export".len();
    }
}

fn collect_default_export(source: &str, cursor: usize, names: &mut BTreeSet<String>) {
    let mut cursor = cursor;
    if let Some(next) = consume_keyword(source, cursor, "async") {
        cursor = skip_ws(source, next);
    }
    if let Some(after_function) = consume_keyword(source, cursor, "function") {
        insert_function_name(source, after_function, names);
    } else if let Some(after_class) = consume_keyword(source, cursor, "class") {
        insert_identifier(source, after_class, names);
    } else {
        insert_identifier(source, cursor, names);
    }
}

fn collect_variable_declarations(source: &str, mut cursor: usize, names: &mut BTreeSet<String>) {
    loop {
        cursor = skip_ws(source, cursor);
        let Some((name, end)) = read_ident(source, cursor) else {
            return;
        };
        names.insert(name.to_string());
        cursor = next_decl_separator(source, end);
        match byte_at(source, cursor) {
            Some(b',') => cursor += 1,
            _ => return,
        }
    }
}

fn collect_export_specifiers(source: &str, start: usize, names: &mut BTreeSet<String>) {
    let Some(end) = find_matching(source, start - 1, b'{', b'}') else {
        return;
    };
    for part in source[start..end].split(',') {
        let words: Vec<&str> = part.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        let local = words[0];
        if local != "default" && is_identifier(local) {
            names.insert(local.to_string());
        }
    }
}

fn collect_commonjs_exports(source: &str, names: &mut BTreeSet<String>) {
    let mut idx = 0;
    while idx < source.len() {
        if keyword_at(source, idx, "module") {
            if let Some(cursor) = consume_dotted_member(source, idx + "module".len(), "exports") {
                collect_commonjs_tail(source, cursor, true, names);
            }
        }
        if keyword_at(source, idx, "exports") && previous_non_ws(source, idx) != Some(b'.') {
            collect_commonjs_tail(source, idx + "exports".len(), false, names);
        }
        idx += 1;
    }
}

fn collect_commonjs_tail(
    source: &str,
    cursor: usize,
    allow_object_assignment: bool,
    names: &mut BTreeSet<String>,
) {
    let cursor = skip_ws(source, cursor);
    if let Some((_, after_property)) = read_export_property(source, cursor) {
        let cursor = skip_ws(source, after_property);
        if is_assignment_at(source, cursor) {
            collect_assignment_rhs(source, cursor + 1, names);
        }
    } else if allow_object_assignment && is_assignment_at(source, cursor) {
        let rhs = skip_ws(source, cursor + 1);
        if byte_at(source, rhs) == Some(b'{') {
            collect_object_export_values(source, rhs + 1, names);
        } else {
            collect_assignment_rhs(source, rhs, names);
        }
    }
}

fn collect_assignment_rhs(source: &str, cursor: usize, names: &mut BTreeSet<String>) {
    let cursor = skip_ws(source, cursor);
    if let Some(after_function) = consume_keyword(source, cursor, "function") {
        insert_function_name(source, after_function, names);
    } else if let Some(after_class) = consume_keyword(source, cursor, "class") {
        insert_identifier(source, after_class, names);
    } else {
        insert_identifier(source, cursor, names);
    }
}

fn collect_object_export_values(source: &str, start: usize, names: &mut BTreeSet<String>) {
    let Some(end) = find_matching(source, start - 1, b'{', b'}') else {
        return;
    };
    for part in split_top_level_commas(&source[start..end]) {
        let part = part.trim();
        if part.is_empty() || part.starts_with("...") {
            continue;
        }
        if let Some(colon) = top_level_colon(part) {
            let rhs = part[colon + 1..].trim_start();
            if let Some((name, _)) = read_ident(rhs, 0) {
                names.insert(name.to_string());
            }
        } else if let Some((name, _)) = read_ident(part, 0) {
            names.insert(name.to_string());
        }
    }
}

fn insert_function_name(source: &str, cursor: usize, names: &mut BTreeSet<String>) {
    let mut cursor = skip_ws(source, cursor);
    if byte_at(source, cursor) == Some(b'*') {
        cursor = skip_ws(source, cursor + 1);
    }
    insert_identifier(source, cursor, names);
}

fn consume_variable_keyword(source: &str, cursor: usize) -> Option<usize> {
    consume_keyword(source, cursor, "const")
        .or_else(|| consume_keyword(source, cursor, "let"))
        .or_else(|| consume_keyword(source, cursor, "var"))
}

fn consume_dotted_member(source: &str, cursor: usize, member: &str) -> Option<usize> {
    let cursor = skip_ws(source, cursor);
    if byte_at(source, cursor) != Some(b'.') {
        return None;
    }
    let cursor = skip_ws(source, cursor + 1);
    consume_keyword(source, cursor, member)
}

fn read_export_property(source: &str, cursor: usize) -> Option<(String, usize)> {
    match byte_at(source, cursor)? {
        b'.' => {
            let cursor = skip_ws(source, cursor + 1);
            let (name, end) = read_ident(source, cursor)?;
            Some((name.to_string(), end))
        }
        b'[' => read_string_literal(source, skip_ws(source, cursor + 1)).and_then(|(name, end)| {
            (byte_at(source, skip_ws(source, end)) == Some(b']'))
                .then_some((name, skip_ws(source, end) + 1))
        }),
        _ => None,
    }
}
