use std::collections::BTreeSet;
use std::path::Path;

use super::scan::{
    byte_at, collect_string_literals, find_matching, is_assignment_at, is_identifier, keyword_at,
    skip_ws, strip_python_line_comment,
};

pub(super) fn is_init(path: &str) -> bool {
    Path::new(path).file_name().and_then(|name| name.to_str()) == Some("__init__.py")
}

pub(super) fn all_names(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut idx = 0;
    while idx < source.len() {
        if !keyword_at(source, idx, "__all__") {
            idx += 1;
            continue;
        }
        let cursor = skip_ws(source, idx + "__all__".len());
        if !is_assignment_at(source, cursor) {
            idx += "__all__".len();
            continue;
        }
        let cursor = skip_ws(source, cursor + 1);
        let (open, close) = match byte_at(source, cursor) {
            Some(b'[') => (b'[', b']'),
            Some(b'(') => (b'(', b')'),
            _ => {
                idx += "__all__".len();
                continue;
            }
        };
        if let Some(end) = find_matching(source, cursor, open, close) {
            collect_string_literals(&source[cursor + 1..end], &mut names);
            idx = end + 1;
        } else {
            idx += "__all__".len();
        }
    }
    names
}

pub(super) fn relative_reexports(path: &str, source: &str) -> Vec<(String, BTreeSet<String>)> {
    let mut reexports = Vec::new();
    for line in source.lines() {
        let line = strip_python_line_comment(line);
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("from ") else {
            continue;
        };
        let Some((module_ref, imports)) = rest.split_once(" import ") else {
            continue;
        };
        let module_ref = module_ref.trim();
        if !module_ref.starts_with('.') || module_ref.starts_with("..") {
            continue;
        }
        let module_ref = module_ref.trim_start_matches('.');
        if module_ref.is_empty() {
            continue;
        }
        let names = imported_names(imports);
        if names.is_empty() {
            continue;
        }
        if let Some(target_path) = relative_module_path(path, module_ref) {
            reexports.push((target_path, names));
        }
    }
    reexports
}

fn imported_names(imports: &str) -> BTreeSet<String> {
    let imports = imports
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();
    let mut names = BTreeSet::new();
    for part in imports.split(',') {
        let part = part.trim();
        if part == "*" || part.is_empty() {
            continue;
        }
        if let Some(name) = part.split_whitespace().next() {
            if is_identifier(name) {
                names.insert(name.to_string());
            }
        }
    }
    names
}

fn relative_module_path(init_path: &str, module_ref: &str) -> Option<String> {
    let package = Path::new(init_path).parent()?.to_str()?.replace('\\', "/");
    let mut path = String::new();
    if !package.is_empty() {
        path.push_str(&package);
        path.push('/');
    }
    path.push_str(&module_ref.replace('.', "/"));
    path.push_str(".py");
    Some(path)
}
