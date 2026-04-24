//! `PublicAPICard` compilation from graph-derived directory facts.
//!
//! Collects exported symbols from direct-child files by reading the
//! `SymbolNode.visibility` field. Entry points are detected using the same
//! four-rule taxonomy as `EntryPointCard`. Recent API changes are symbols
//! whose containing file was last touched within 30 days (`Deep` budget only).
//!
//! # Cross-language visibility
//!
//! `PublicAPICard` now emits symbols for Rust, Python, TypeScript, and Go.
//! - Rust: `pub` -> Public, `pub(crate)` -> Crate, no prefix -> Private.
//! - Python: dunders and non-underscore names -> Public, `_name` -> Private.
//! - TypeScript: wrapped in `export` -> Public, otherwise -> Public (v1).
//! - Go: uppercase first char -> Public, lowercase -> Private.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    core::ids::FileNodeId,
    structure::graph::{GraphReader, Visibility},
    surface::card::{
        git::symbol_last_change_from_insights,
        types::{PublicAPICard, PublicAPIEntry},
        Budget, ContextAccounting, SourceStore,
    },
};

use super::GraphCardCompiler;

/// Number of days that qualifies a public symbol change as "recent".
const RECENT_API_DAYS: i64 = 30;

/// Compile a `PublicAPICard` for the given directory path.
pub(super) fn public_api_card_impl(
    compiler: &GraphCardCompiler,
    graph: &dyn GraphReader,
    path: &str,
    budget: Budget,
) -> crate::Result<PublicAPICard> {
    // Normalise: ensure the prefix ends with `/` for correct child matching.
    let prefix = if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{path}/")
    };

    let all_paths = graph.all_file_paths()?;

    // Collect direct-child file paths (not deeper descendants).
    let mut direct_files: Vec<(String, FileNodeId)> = Vec::new();
    for (file_path, file_id) in &all_paths {
        let Some(suffix) = file_path.strip_prefix(&prefix) else {
            continue;
        };
        if suffix.is_empty() || suffix.contains('/') {
            continue;
        }
        direct_files.push((file_path.clone(), *file_id));
    }
    direct_files.sort_by(|a, b| a.0.cmp(&b.0));

    let include_git = budget != Budget::Tiny;
    let include_summary = budget == Budget::Deep;
    let now = now_unix();

    let mut public_symbols: Vec<PublicAPIEntry> = Vec::new();
    let mut public_symbol_count: usize = 0;

    for (file_path, file_id) in &direct_files {
        let git_insights = if include_git {
            compiler.resolve_file_git_intelligence(file_path)
        } else {
            None
        };

        let symbols = graph.symbols_for_file(*file_id)?;
        for sym in &symbols {
            // Visibility filter: include Public and Crate, exclude Private and Unknown.
            let is_visible = matches!(sym.visibility, Visibility::Public | Visibility::Crate);
            if !is_visible {
                continue;
            }

            // Signature is optional but we use it if present for the entry.
            let sig = sym.signature.clone().unwrap_or_default();

            public_symbol_count += 1;

            if budget == Budget::Tiny {
                // Count only; don't materialise entries.
                continue;
            }

            let last_change = git_insights
                .as_ref()
                .and_then(|arc| symbol_last_change_from_insights(arc, include_summary, None));

            public_symbols.push(PublicAPIEntry {
                id: sym.id,
                name: sym.display_name.clone(),
                kind: sym.kind,
                signature: sig,
                location: format!("{}:{}", file_path, sym.body_byte_range.0),
                last_change,
            });
        }
    }

    // Public entry points: public symbols that also match an entry-point rule.
    let public_entry_points: Vec<PublicAPIEntry> = if budget == Budget::Tiny {
        vec![]
    } else {
        public_symbols
            .iter()
            .filter(|e| super::entry_point::classify_kind(&e.name, &e.location, e.kind).is_some())
            .cloned()
            .collect()
    };

    // Recent API changes (Deep only): public symbols last touched within 30 days.
    let recent_api_changes: Vec<PublicAPIEntry> = if budget == Budget::Deep {
        public_symbols
            .iter()
            .filter(|e| {
                e.last_change
                    .as_ref()
                    .map(|lc| lc.committed_at_unix > now - RECENT_API_DAYS * 86400)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    } else {
        vec![]
    };

    let per_symbol: usize = match budget {
        Budget::Tiny => 10,
        Budget::Normal => 30,
        Budget::Deep => 60,
    };
    let approx_tokens = public_symbol_count * per_symbol + 20;

    Ok(PublicAPICard {
        path: prefix,
        public_symbols,
        public_symbol_count,
        public_entry_points,
        recent_api_changes,
        approx_tokens,
        context_accounting: ContextAccounting::new(budget, approx_tokens, 0, vec![]),
        source_store: SourceStore::Graph,
    })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod public_api_tests;
