//! Conservative parsing of direct file-reader shell commands for read hints.
//!
//! Kept dependency-free and deliberately narrow: only single-file, unredirected
//! reader invocations resolve to a path. Anything ambiguous returns `None`.

use super::super::classify;

pub(super) fn extract_direct_reader_path(command: &str) -> Option<String> {
    let command = classify::strip_rtk_prefix(command);
    if command_has_shell_control(command) {
        return None;
    }
    let tokens = shell_words(command)?;
    let (tool, rest) = tokens.split_first()?;
    let candidates = match tool.as_str() {
        "cat" | "nl" | "less" | "bat" => simple_reader_candidates(rest),
        "head" | "tail" => counted_reader_candidates(rest),
        "sed" => sed_reader_candidates(rest),
        _ => return None,
    };
    if candidates.len() == 1 && path_token_is_safe(&candidates[0]) {
        Some(candidates[0].clone())
    } else {
        None
    }
}

fn simple_reader_candidates(tokens: &[String]) -> Vec<String> {
    tokens
        .iter()
        .filter(|token| !token.starts_with('-'))
        .cloned()
        .collect()
}

fn counted_reader_candidates(tokens: &[String]) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut skip_next = false;
    for token in tokens {
        if skip_next {
            skip_next = false;
            continue;
        }
        if matches!(token.as_str(), "-n" | "-c") {
            skip_next = true;
            continue;
        }
        if token.starts_with('-') {
            continue;
        }
        candidates.push(token.clone());
    }
    candidates
}

fn sed_reader_candidates(tokens: &[String]) -> Vec<String> {
    // `sed -i` / `--in-place` mutates the file; it is a write, not a read.
    if tokens.iter().any(|token| token_is_sed_in_place(token)) {
        return Vec::new();
    }
    let mut non_options = Vec::new();
    let mut skip_next = false;
    for token in tokens {
        if skip_next {
            skip_next = false;
            continue;
        }
        if matches!(token.as_str(), "-e" | "-f") {
            skip_next = true;
            continue;
        }
        if token.starts_with('-') {
            continue;
        }
        non_options.push(token.clone());
    }
    if non_options.len() == 2 {
        vec![non_options[1].clone()]
    } else {
        Vec::new()
    }
}

fn token_is_sed_in_place(token: &str) -> bool {
    if let Some(long) = token.strip_prefix("--") {
        return long.starts_with("in-place");
    }
    // Short-option bundle (`-i`, `-i.bak`, `-ni`): `-i` is the only
    // read-relevant sed option whose letters include 'i'.
    token
        .strip_prefix('-')
        .is_some_and(|short| short.contains('i'))
}

fn command_has_shell_control(command: &str) -> bool {
    command.contains('|')
        || command.contains(';')
        || command.contains('<')
        || command.contains('>')
        || command.contains("&&")
        || command.contains("||")
        || command.contains("$(")
        || command.contains('`')
        || command.contains('\n')
}

fn shell_words(command: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in command.chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '\'' | '"') => quote = Some(ch),
            (None, c) if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            (None, c) => current.push(c),
        }
    }
    if quote.is_some() {
        return None;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Some(tokens)
}

pub(super) fn path_token_is_safe(token: &str) -> bool {
    !token.is_empty()
        && !token.starts_with('-')
        && !token.contains('\0')
        && !token
            .chars()
            .any(|c| matches!(c, '*' | '?' | '[' | ']' | '{' | '}' | '$'))
}
