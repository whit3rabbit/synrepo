//! Graph-backed prompt context for explain commentary generation.

use std::fs;
use std::path::Path;

use crate::core::ids::NodeId;
use crate::pipeline::explain::commentary_template::{
    build_commentary_context, estimate_context_tokens,
};
use crate::structure::graph::{FileNode, GraphReader, SymbolNode};

mod blocks;
mod describe;
#[cfg(test)]
mod tests;

use blocks::optional_blocks;

const DEFAULT_MAX_INPUT_TOKENS: u32 = 5_000;
const MAX_SOURCE_CHARS: usize = 16_000;

/// Options for commentary context assembly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommentaryContextOptions {
    /// Maximum approximate input tokens to send to the commentary provider.
    pub max_input_tokens: u32,
    /// Graph expansion degree. V1 supports only 0 or 1; values above 1 clamp to 1.
    pub graph_degree: u8,
}

impl Default for CommentaryContextOptions {
    fn default() -> Self {
        Self {
            max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
            graph_degree: 1,
        }
    }
}

/// Graph-side commentary target resolved before prompt assembly.
#[derive(Clone, Debug)]
pub struct CommentaryContextTarget {
    /// Content hash used for commentary freshness.
    pub content_hash: String,
    /// File described by the commentary, or containing the symbol target.
    pub file: FileNode,
    /// Present for symbol commentary; `None` for file commentary.
    pub symbol: Option<SymbolNode>,
}

impl CommentaryContextTarget {
    /// Build a target from graph-resolved parts.
    pub fn new(content_hash: String, file: FileNode, symbol: Option<SymbolNode>) -> Self {
        Self {
            content_hash,
            file,
            symbol,
        }
    }

    /// Node ID this commentary describes.
    pub fn node_id(&self) -> NodeId {
        self.symbol
            .as_ref()
            .map(|symbol| NodeId::Symbol(symbol.id))
            .unwrap_or(NodeId::File(self.file.id))
    }

    /// Qualified symbol name when this is symbol commentary.
    pub fn qualified_name(&self) -> String {
        self.symbol
            .as_ref()
            .map(|symbol| symbol.qualified_name.clone())
            .unwrap_or_default()
    }
}

/// Resolve a file or symbol node into a commentary context target.
pub fn resolve_context_target(
    graph: &dyn GraphReader,
    node: NodeId,
) -> crate::Result<Option<CommentaryContextTarget>> {
    match node {
        NodeId::File(file_id) => Ok(graph
            .get_file(file_id)?
            .map(|file| CommentaryContextTarget::new(file.content_hash.clone(), file, None))),
        NodeId::Symbol(sym_id) => {
            let Some(symbol) = graph.get_symbol(sym_id)? else {
                return Ok(None);
            };
            let Some(file) = graph.get_file(symbol.file_id)? else {
                return Ok(None);
            };
            Ok(Some(CommentaryContextTarget::new(
                file.content_hash.clone(),
                file,
                Some(symbol),
            )))
        }
        NodeId::Concept(_) => Ok(None),
    }
}

/// Build the prompt context passed to commentary providers.
pub fn build_context_text(
    repo_root: &Path,
    graph: &dyn GraphReader,
    target: &CommentaryContextTarget,
    options: CommentaryContextOptions,
) -> String {
    let target_summary = target_summary(target);
    let optional = if options.graph_degree == 0 {
        Vec::new()
    } else {
        optional_blocks(repo_root, graph, target)
    };
    build_budgeted_context(repo_root, target, &target_summary, &optional, options)
}

fn build_budgeted_context(
    repo_root: &Path,
    target: &CommentaryContextTarget,
    target_summary: &str,
    optional: &[String],
    options: CommentaryContextOptions,
) -> String {
    let budget = options.max_input_tokens.max(1);
    let source_limits = [MAX_SOURCE_CHARS, 8_000, 4_000, 2_000, 1_000, 0];
    // Read once and re-truncate per iteration. The fitting loop walks up to
    // 6 source-limit tiers, each with up to N optional-block reductions, so
    // re-reading the file each time would issue ~50+ identical fs reads.
    let source_full = read_target_source_raw(repo_root, target);
    let mut included = optional.len();
    let mut last_prompt = String::new();

    for source_limit in source_limits {
        loop {
            let evidence = evidence_context(
                target,
                source_full.as_deref(),
                source_limit,
                &optional[..included],
            );
            let prompt = build_commentary_context(target_summary, &evidence);
            if estimate_context_tokens(&prompt) <= budget {
                return prompt;
            }
            last_prompt = prompt;
            if included == 0 {
                break;
            }
            included -= 1;
        }
    }

    last_prompt
}

fn target_summary(target: &CommentaryContextTarget) -> String {
    let mut out = format!("Target node: {}\n", target.node_id());
    match &target.symbol {
        Some(symbol) => {
            out.push_str("Target kind: symbol\n");
            out.push_str(&format!("Symbol: {}\n", symbol.display_name));
            out.push_str(&format!("Qualified name: {}\n", symbol.qualified_name));
            out.push_str(&format!("Source path: {}\n", target.file.path));
            out.push_str(&format!("Symbol kind: {}\n", symbol.kind.as_str()));
            out.push_str(&format!("Visibility: {}\n", symbol.visibility.as_str()));
            if let Some(signature) = &symbol.signature {
                out.push_str(&format!("Signature: {signature}\n"));
            }
        }
        None => {
            out.push_str("Target kind: file\n");
            out.push_str(&format!("Source path: {}\n", target.file.path));
            if let Some(language) = &target.file.language {
                out.push_str(&format!("Language: {language}\n"));
            }
            out.push_str(
                "Only explain this file. Use related files as context, not as the target.\n",
            );
        }
    }
    out
}

fn evidence_context(
    target: &CommentaryContextTarget,
    source_full: Option<&str>,
    source_limit: usize,
    optional: &[String],
) -> String {
    let mut out = String::new();
    if let Some(symbol) = &target.symbol {
        if let Some(doc) = &symbol.doc_comment {
            out.push_str(&format!("<doc_comment>\n{doc}\n</doc_comment>\n"));
        }
    }

    if source_limit > 0 {
        match source_full {
            Some(source) => out.push_str(&format!(
                "<source_code path=\"{}\">\n{}\n</source_code>\n",
                target.file.path,
                truncate_chars(source, source_limit)
            )),
            None => out.push_str(&format!(
                "<source_code path=\"{}\">\nunavailable\n</source_code>\n",
                target.file.path
            )),
        }
    }

    for block in optional {
        out.push_str(block);
    }
    out
}

fn read_target_source_raw(repo_root: &Path, target: &CommentaryContextTarget) -> Option<String> {
    let text = fs::read_to_string(repo_root.join(&target.file.path)).ok()?;
    let raw = match &target.symbol {
        Some(symbol) => {
            let bytes = text.as_bytes();
            let start = (symbol.body_byte_range.0 as usize).min(bytes.len());
            let end = (symbol.body_byte_range.1 as usize)
                .min(bytes.len())
                .max(start);
            String::from_utf8_lossy(&bytes[start..end]).to_string()
        }
        None => text,
    };
    Some(raw)
}

pub(super) fn truncate_chars(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut truncated = text.chars().take(limit).collect::<String>();
    truncated.push_str("\n/* truncated */");
    truncated
}
