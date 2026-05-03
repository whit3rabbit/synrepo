use std::collections::HashMap;

use super::{default_limit, handle_context_pack, handle_file_outline_resource, ContextPackParams};
use crate::surface::mcp::compact::OutputMode;
use crate::surface::mcp::SynrepoState;

pub fn read_resource(state: &SynrepoState, uri: &str) -> Result<String, String> {
    let Some(rest) = uri.strip_prefix("synrepo://") else {
        return Err(format!("unsupported synrepo resource URI: {uri}"));
    };
    let (path, query) = split_resource_query(rest);
    let params = parse_resource_query(query);
    let budget = params
        .get("budget")
        .cloned()
        .unwrap_or_else(|| "tiny".to_string());

    if let Some(target) = path.strip_prefix("card/") {
        return Ok(super::super::cards::handle_card(
            state,
            decode_resource_component(target),
            budget,
            None,
            false,
        ));
    }
    if let Some(path) = path
        .strip_prefix("file/")
        .and_then(|p| p.strip_suffix("/outline"))
    {
        return Ok(handle_file_outline_resource(
            state,
            decode_resource_component(path),
            budget,
        ));
    }
    if path == "context-pack" {
        return Ok(handle_context_pack(
            state,
            ContextPackParams {
                repo_root: None,
                goal: params.get("goal").cloned(),
                targets: Vec::new(),
                budget,
                budget_tokens: params
                    .get("budget_tokens")
                    .and_then(|value| value.parse::<usize>().ok()),
                output_mode: params
                    .get("output_mode")
                    .map(|value| {
                        if value == "compact" {
                            OutputMode::Compact
                        } else {
                            OutputMode::Default
                        }
                    })
                    .unwrap_or_default(),
                include_tests: false,
                include_notes: false,
                limit: params
                    .get("limit")
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or_else(default_limit),
            },
        ));
    }
    Err(format!("unknown synrepo resource URI: {uri}"))
}

fn split_resource_query(input: &str) -> (&str, &str) {
    input.split_once('?').unwrap_or((input, ""))
}

fn parse_resource_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (
                decode_resource_component(key),
                decode_resource_component(value),
            )
        })
        .collect()
}

fn decode_resource_component(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(value) = u8::from_str_radix(hex, 16) {
                    out.push(value);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
