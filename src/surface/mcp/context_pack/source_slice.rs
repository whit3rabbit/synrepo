use serde_json::{json, Value};

use crate::{
    core::path_safety::safe_join_in_repo,
    structure::graph::{FileNode, GraphReader, SymbolNode},
    surface::{
        card::{
            accounting::{estimate_tokens_bytes, ContextAccounting},
            compiler::{resolve_target, GraphCardCompiler},
            Budget,
        },
        mcp::context_pack::ContextPackTarget,
    },
    NodeId,
};

struct ResolvedSliceTarget {
    file: FileNode,
    symbols: Vec<SymbolNode>,
}

struct SourceSection {
    start_line: usize,
    end_line: usize,
    symbols: Vec<String>,
}

pub(super) fn source_slice_content(
    compiler: &GraphCardCompiler,
    target: &ContextPackTarget,
    budget: Budget,
    budget_tokens: Option<usize>,
) -> crate::Result<Value> {
    let resolved = compiler.with_reader(|graph| resolve_slice_target(graph, target, budget))?;
    let Some(source_root) = compiler.source_root_for(&resolved.file.root_id) else {
        return Ok(stale_content(
            target,
            &resolved.file,
            budget,
            "source_root_unavailable",
            0,
        ));
    };
    let Some(path) = safe_join_in_repo(&source_root, &resolved.file.path) else {
        return Ok(stale_content(
            target,
            &resolved.file,
            budget,
            "unsafe_repo_path",
            0,
        ));
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Ok(stale_content(
                target,
                &resolved.file,
                budget,
                "source_unreadable",
                0,
            ));
        }
    };
    let current_hash = hex::encode(blake3::hash(&bytes).as_bytes());
    let raw_tokens = estimate_tokens_bytes(bytes.len());
    if current_hash != resolved.file.content_hash {
        return Ok(stale_content(
            target,
            &resolved.file,
            budget,
            "content_hash_mismatch",
            raw_tokens,
        ));
    }

    let source = String::from_utf8_lossy(&bytes);
    let mut sections = slice_sections(&source, &resolved.symbols, budget);
    let char_cap = char_cap(budget, budget_tokens);
    let (files, token_estimate, truncated) =
        render_file_sections(&resolved.file, &source, &mut sections, char_cap);
    let mut accounting = ContextAccounting::new(
        budget,
        token_estimate,
        raw_tokens,
        vec![resolved.file.content_hash.clone()],
    );
    accounting.truncation_applied = truncated;

    Ok(json!({
        "source_store": "source",
        "slice_state": "fresh",
        "line_numbers": "one_based_tab_prefixed",
        "files": files,
        "omitted": [],
        "context_accounting": accounting,
    }))
}

fn resolve_slice_target(
    graph: &dyn GraphReader,
    target: &ContextPackTarget,
    budget: Budget,
) -> crate::Result<ResolvedSliceTarget> {
    let node = resolve_target(graph, &target.target)?.ok_or_else(|| {
        crate::Error::Other(anyhow::anyhow!("target not found: {}", target.target))
    })?;
    match node {
        NodeId::File(file_id) => {
            let file = graph
                .get_file(file_id)?
                .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("file {file_id} not found")))?;
            let mut symbols = graph.symbols_for_file(file_id)?;
            symbols.sort_by_key(|symbol| symbol.body_byte_range.0);
            symbols.truncate(symbol_limit(budget));
            Ok(ResolvedSliceTarget { file, symbols })
        }
        NodeId::Symbol(symbol_id) => {
            let symbol = graph.get_symbol(symbol_id)?.ok_or_else(|| {
                crate::Error::Other(anyhow::anyhow!("symbol {symbol_id} not found"))
            })?;
            let file = graph.get_file(symbol.file_id)?.ok_or_else(|| {
                crate::Error::Other(anyhow::anyhow!("file for symbol {symbol_id} not found"))
            })?;
            Ok(ResolvedSliceTarget {
                file,
                symbols: vec![symbol],
            })
        }
        NodeId::Concept(_) => Err(crate::Error::Other(anyhow::anyhow!(
            "source_slice target must resolve to a file or symbol"
        ))),
    }
}

fn stale_content(
    target: &ContextPackTarget,
    file: &FileNode,
    budget: Budget,
    reason: &str,
    raw_tokens: usize,
) -> Value {
    let mut accounting = ContextAccounting::new(
        budget,
        estimate_tokens_bytes(reason.len()),
        raw_tokens,
        vec![],
    );
    accounting.stale = true;
    json!({
        "source_store": "source",
        "slice_state": "stale_omitted",
        "line_numbers": "one_based_tab_prefixed",
        "files": [],
        "omitted": [{
            "target": target.target,
            "path": file.path,
            "reason": reason,
            "expected_content_hash": file.content_hash,
        }],
        "context_accounting": accounting,
    })
}

fn symbol_limit(budget: Budget) -> usize {
    match budget {
        Budget::Tiny => 3,
        Budget::Normal => 8,
        Budget::Deep => 20,
    }
}

fn char_cap(budget: Budget, budget_tokens: Option<usize>) -> usize {
    let base = match budget {
        Budget::Tiny => 1_800,
        Budget::Normal => 4_500,
        Budget::Deep => 9_000,
    };
    budget_tokens
        .map(|tokens| base.min(tokens.saturating_mul(3)).max(300))
        .unwrap_or(base)
}

fn slice_sections(source: &str, symbols: &[SymbolNode], budget: Budget) -> Vec<SourceSection> {
    let line_count = source.lines().count().max(1);
    if symbols.is_empty() {
        let end_line = match budget {
            Budget::Tiny => 30,
            Budget::Normal => 80,
            Budget::Deep => 160,
        }
        .min(line_count);
        return vec![SourceSection {
            start_line: 1,
            end_line,
            symbols: Vec::new(),
        }];
    }

    let starts = line_starts(source);
    let padding = match budget {
        Budget::Tiny => 1,
        Budget::Normal => 2,
        Budget::Deep => 4,
    };
    let gap = match budget {
        Budget::Tiny => 2,
        Budget::Normal => 4,
        Budget::Deep => 8,
    };
    let mut ranges = symbols
        .iter()
        .map(|symbol| {
            let start = byte_to_line(&starts, symbol.body_byte_range.0 as usize)
                .saturating_sub(padding)
                .max(1);
            let end_byte = (symbol.body_byte_range.1 as usize).saturating_sub(1);
            let end = (byte_to_line(&starts, end_byte) + padding).min(line_count);
            SourceSection {
                start_line: start,
                end_line: end.max(start),
                symbols: vec![symbol.qualified_name.clone()],
            }
        })
        .collect::<Vec<_>>();
    ranges.sort_by_key(|section| section.start_line);

    let mut merged: Vec<SourceSection> = Vec::new();
    for section in ranges {
        if let Some(last) = merged.last_mut() {
            if section.start_line <= last.end_line.saturating_add(gap) {
                last.end_line = last.end_line.max(section.end_line);
                last.symbols.extend(section.symbols);
                continue;
            }
        }
        merged.push(section);
    }
    merged
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (idx, byte) in source.as_bytes().iter().enumerate() {
        if *byte == b'\n' {
            starts.push(idx + 1);
        }
    }
    starts
}

fn byte_to_line(starts: &[usize], byte: usize) -> usize {
    match starts.binary_search(&byte) {
        Ok(idx) => idx + 1,
        Err(idx) => idx.max(1),
    }
}

fn render_file_sections(
    file: &FileNode,
    source: &str,
    sections: &mut [SourceSection],
    char_cap: usize,
) -> (Vec<Value>, usize, bool) {
    let mut rendered = String::new();
    let mut section_values = Vec::new();
    let mut truncated = false;
    let lines = source.lines().collect::<Vec<_>>();
    let mut remaining = char_cap;
    let mut previous_end = 0usize;

    for section in sections {
        if remaining == 0 {
            truncated = true;
            break;
        }
        if previous_end > 0 && section.start_line > previous_end + 1 {
            let marker = format!(
                "... lines {}-{} omitted ...\n",
                previous_end + 1,
                section.start_line - 1
            );
            append_capped(&mut rendered, &marker, &mut remaining, &mut truncated);
        }
        for line_no in section.start_line..=section.end_line {
            let Some(line) = lines.get(line_no.saturating_sub(1)) else {
                continue;
            };
            let numbered = format!("{line_no}\t{line}\n");
            append_capped(&mut rendered, &numbered, &mut remaining, &mut truncated);
            if remaining == 0 {
                break;
            }
        }
        section_values.push(json!({
            "start_line": section.start_line,
            "end_line": section.end_line,
            "symbols": section.symbols,
        }));
        previous_end = section.end_line;
        if remaining == 0 {
            truncated = true;
            break;
        }
    }

    let files = vec![json!({
        "path": file.path,
        "file_id": file.id,
        "root_id": file.root_id,
        "content_hash": file.content_hash,
        "sections": section_values,
        "rendered_source": rendered,
        "truncated": truncated,
    })];
    (
        files,
        estimate_tokens_bytes(char_cap.saturating_sub(remaining)),
        truncated,
    )
}

fn append_capped(out: &mut String, text: &str, remaining: &mut usize, truncated: &mut bool) {
    if *remaining == 0 {
        *truncated = true;
        return;
    }
    if text.len() <= *remaining {
        out.push_str(text);
        *remaining -= text.len();
        return;
    }
    let mut end = *remaining;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    out.push_str(&text[..end]);
    *remaining = 0;
    *truncated = true;
}
