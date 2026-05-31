use std::collections::BTreeSet;

pub(super) fn consume_keyword(source: &str, cursor: usize, keyword: &str) -> Option<usize> {
    keyword_at(source, cursor, keyword).then_some(cursor + keyword.len())
}

pub(super) fn keyword_at(source: &str, cursor: usize, keyword: &str) -> bool {
    source
        .get(cursor..)
        .is_some_and(|tail| tail.starts_with(keyword))
        && (cursor == 0 || !is_ident_continue(source.as_bytes()[cursor - 1]))
        && byte_at(source, cursor + keyword.len()).is_none_or(|byte| !is_ident_continue(byte))
}

pub(super) fn read_ident(source: &str, cursor: usize) -> Option<(&str, usize)> {
    let bytes = source.as_bytes();
    let first = *bytes.get(cursor)?;
    if !is_ident_start(first) {
        return None;
    }
    let mut end = cursor + 1;
    while bytes.get(end).is_some_and(|byte| is_ident_continue(*byte)) {
        end += 1;
    }
    Some((&source[cursor..end], end))
}

pub(super) fn insert_identifier(source: &str, cursor: usize, names: &mut BTreeSet<String>) {
    if let Some((name, _)) = read_ident(source, skip_ws(source, cursor)) {
        if !matches!(
            name,
            "class" | "function" | "require" | "undefined" | "null" | "true" | "false"
        ) {
            names.insert(name.to_string());
        }
    }
}

pub(super) fn is_identifier(value: &str) -> bool {
    let Some(first) = value.as_bytes().first().copied() else {
        return false;
    };
    is_ident_start(first)
        && value
            .as_bytes()
            .iter()
            .skip(1)
            .all(|byte| is_ident_continue(*byte))
}

fn is_ident_start(byte: u8) -> bool {
    byte == b'_' || byte == b'$' || byte.is_ascii_alphabetic()
}

fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}

pub(super) fn skip_ws(source: &str, mut cursor: usize) -> usize {
    while byte_at(source, cursor).is_some_and(|byte| byte.is_ascii_whitespace()) {
        cursor += 1;
    }
    cursor
}

pub(super) fn byte_at(source: &str, cursor: usize) -> Option<u8> {
    source.as_bytes().get(cursor).copied()
}

pub(super) fn previous_non_ws(source: &str, cursor: usize) -> Option<u8> {
    source.as_bytes()[..cursor]
        .iter()
        .rev()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
}

pub(super) fn is_assignment_at(source: &str, cursor: usize) -> bool {
    byte_at(source, cursor) == Some(b'=')
        && byte_at(source, cursor + 1) != Some(b'=')
        && byte_at(source, cursor + 1) != Some(b'>')
        && cursor.checked_sub(1).and_then(|prev| byte_at(source, prev)) != Some(b'=')
}

pub(super) fn next_decl_separator(source: &str, mut cursor: usize) -> usize {
    let mut depth = 0usize;
    let mut quote = None;
    while cursor < source.len() {
        let byte = source.as_bytes()[cursor];
        if let Some(end_quote) = quote {
            if byte == b'\\' {
                cursor += 2;
                continue;
            }
            if byte == end_quote {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' | b'`' => quote = Some(byte),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' | b';' | b'\n' if depth == 0 => break,
            _ => {}
        }
        cursor += 1;
    }
    cursor
}

pub(super) fn split_top_level_commas(source: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut cursor = 0;
    let mut depth = 0usize;
    let mut quote = None;
    while cursor < source.len() {
        let byte = source.as_bytes()[cursor];
        if let Some(end_quote) = quote {
            if byte == b'\\' {
                cursor += 2;
                continue;
            }
            if byte == end_quote {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' | b'`' => quote = Some(byte),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => {
                    parts.push(&source[start..cursor]);
                    start = cursor + 1;
                }
                _ => {}
            }
        }
        cursor += 1;
    }
    parts.push(&source[start..]);
    parts
}

pub(super) fn top_level_colon(source: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut depth = 0usize;
    let mut quote = None;
    while cursor < source.len() {
        let byte = source.as_bytes()[cursor];
        if let Some(end_quote) = quote {
            if byte == b'\\' {
                cursor += 2;
                continue;
            }
            if byte == end_quote {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' | b'`' => quote = Some(byte),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                b':' if depth == 0 => return Some(cursor),
                _ => {}
            }
        }
        cursor += 1;
    }
    None
}

pub(super) fn find_matching(source: &str, open_at: usize, open: u8, close: u8) -> Option<usize> {
    if byte_at(source, open_at) != Some(open) {
        return None;
    }
    let mut depth = 0usize;
    let mut cursor = open_at;
    let mut quote = None;
    while cursor < source.len() {
        let byte = source.as_bytes()[cursor];
        if let Some(end_quote) = quote {
            if byte == b'\\' {
                cursor += 2;
                continue;
            }
            if byte == end_quote {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' | b'`' => quote = Some(byte),
            b if b == open => depth += 1,
            b if b == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(cursor);
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

pub(super) fn read_string_literal(source: &str, cursor: usize) -> Option<(String, usize)> {
    let quote = byte_at(source, cursor)?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let mut value = String::new();
    let mut idx = cursor + 1;
    while idx < source.len() {
        let byte = source.as_bytes()[idx];
        if byte == b'\\' {
            idx += 1;
            if let Some(escaped) = byte_at(source, idx) {
                value.push(escaped as char);
            }
        } else if byte == quote {
            return Some((value, idx + 1));
        } else {
            value.push(byte as char);
        }
        idx += 1;
    }
    None
}

pub(super) fn collect_string_literals(source: &str, names: &mut BTreeSet<String>) {
    let mut cursor = 0;
    while cursor < source.len() {
        if matches!(byte_at(source, cursor), Some(b'\'') | Some(b'"')) {
            if let Some((value, end)) = read_string_literal(source, cursor) {
                names.insert(value);
                cursor = end;
                continue;
            }
        }
        cursor += 1;
    }
}

pub(super) fn strip_js_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    let mut state = JsState::Code;
    while idx < bytes.len() {
        let byte = bytes[idx];
        match state {
            JsState::Code => {
                if byte == b'/' && bytes.get(idx + 1) == Some(&b'/') {
                    output.extend_from_slice(b"  ");
                    idx += 2;
                    state = JsState::LineComment;
                } else if byte == b'/' && bytes.get(idx + 1) == Some(&b'*') {
                    output.extend_from_slice(b"  ");
                    idx += 2;
                    state = JsState::BlockComment;
                } else {
                    output.push(byte);
                    state = match byte {
                        b'\'' => JsState::SingleQuote,
                        b'"' => JsState::DoubleQuote,
                        b'`' => JsState::Template,
                        _ => JsState::Code,
                    };
                    idx += 1;
                }
            }
            JsState::LineComment => {
                output.push(if byte == b'\n' { b'\n' } else { b' ' });
                if byte == b'\n' {
                    state = JsState::Code;
                }
                idx += 1;
            }
            JsState::BlockComment => {
                if byte == b'*' && bytes.get(idx + 1) == Some(&b'/') {
                    output.extend_from_slice(b"  ");
                    idx += 2;
                    state = JsState::Code;
                } else {
                    output.push(if byte == b'\n' { b'\n' } else { b' ' });
                    idx += 1;
                }
            }
            JsState::SingleQuote | JsState::DoubleQuote | JsState::Template => {
                output.push(byte);
                if byte == b'\\' {
                    idx += 1;
                    if let Some(next) = bytes.get(idx) {
                        output.push(*next);
                    }
                } else if state.ends_on(byte) {
                    state = JsState::Code;
                }
                idx += 1;
            }
        }
    }
    String::from_utf8(output).unwrap_or_else(|_| source.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JsState {
    Code,
    LineComment,
    BlockComment,
    SingleQuote,
    DoubleQuote,
    Template,
}

impl JsState {
    fn ends_on(self, byte: u8) -> bool {
        matches!(
            (self, byte),
            (JsState::SingleQuote, b'\'')
                | (JsState::DoubleQuote, b'"')
                | (JsState::Template, b'`')
        )
    }
}

pub(super) fn strip_python_line_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quote = None;
    let mut cursor = 0;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if let Some(end_quote) = quote {
            if byte == b'\\' {
                cursor += 2;
                continue;
            }
            if byte == end_quote {
                quote = None;
            }
        } else if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
        } else if byte == b'#' {
            return &line[..cursor];
        }
        cursor += 1;
    }
    line
}
