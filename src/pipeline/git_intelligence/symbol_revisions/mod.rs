//! Derive per-symbol `first_seen_rev` and `last_modified_rev` from body-hash
//! transitions across sampled commit history.
//!
//! Stage 5 extension: walks sampled first-parent commits per file, parses the
//! file at each revision, and compares body_hash values for matching qualified
//! names to determine when each symbol was first seen and last modified.

use std::collections::HashMap;

use crate::structure::graph::GraphStore;
use crate::structure::parse::parse_file;
use std::path::Path;

/// Key for matching symbols across commits: (qualified_name, kind_label).
type SymbolKey = (String, String);

/// Derive symbol-scoped revisions for all files with sampled history.
///
/// For each file that appears in the sampled commits, parses the file at each
/// sampled revision and diffs body_hash values against the current compile.
/// Writes the derived revisions back to the graph store.
pub fn derive_symbol_revisions(
    repo_root: &Path,
    context: &crate::pipeline::git::GitIntelligenceContext,
    graph: &mut dyn GraphStore,
    max_commits: usize,
) -> crate::Result<()> {
    if context.repository().is_degraded() {
        return Ok(());
    }

    let index = super::index::GitHistoryIndex::build(context, max_commits)?;
    let change_sets = index.change_sets();

    // Build file_id -> file_path reverse map.
    let file_paths = graph.all_file_paths()?;
    let id_to_path: HashMap<_, _> = file_paths.iter().map(|(p, id)| (id, p.clone())).collect();

    // Load all current symbols grouped by file path, keyed by (qualified_name, kind).
    let symbol_names = graph.all_symbol_names()?;
    let mut current_by_file: HashMap<String, HashMap<SymbolKey, (u64, String)>> = HashMap::new();
    for (sym_id, file_id, _qname) in &symbol_names {
        let Some(path) = id_to_path.get(&file_id) else {
            continue;
        };
        let Ok(Some(sym)) = graph.get_symbol(*sym_id) else {
            continue;
        };
        let key = (sym.qualified_name.clone(), sym.kind.as_str().to_string());
        current_by_file
            .entry(path.clone())
            .or_default()
            .insert(key, (sym_id.0, sym.body_hash.clone()));
    }

    for (file_path, current_hashes) in &current_by_file {
        // Only process files that have sampled commits.
        let touched_indices = match index.by_path().get(file_path.as_str()) {
            Some(indices) => indices,
            None => continue,
        };

        // Walk commits newest-to-oldest. For each symbol seen:
        // - first_seen_rev: the *oldest* commit where the name appears (last
        //   assignment wins since we walk backwards).
        // - last_modified_rev: the *newest* commit where body_hash differs from
        //   the current value (first such assignment wins).
        let repo = match crate::pipeline::git::open_repo(repo_root) {
        Ok(r) => r,
        Err(_) => return Ok(()),
    };

        let mut first_seen: HashMap<SymbolKey, String> = HashMap::new();
        let mut last_modified: HashMap<SymbolKey, String> = HashMap::new();

        for &idx in touched_indices {
            let rev = change_sets[idx].commit.revision.clone();
            let Some(content) =
                crate::pipeline::git::file_content_at_revision(&repo, &rev, file_path)
            else {
                continue;
            };

            for (key, hash) in parse_symbols_for_hashes(file_path, &content) {
                // Overwrite on each iteration; final value is the oldest commit.
                first_seen.insert(key.clone(), rev.clone());

                if let Some((_, current_hash)) = current_hashes.get(&key) {
                    if &hash != current_hash && !last_modified.contains_key(&key) {
                        last_modified.insert(key, rev.clone());
                    }
                }
            }
        }

        // Write derived revisions back to graph store.
        for (key, (sym_id_u64, _)) in current_hashes {
            let first = first_seen.get(key).cloned();
            let last_mod = last_modified.get(key).cloned();

            if first.is_some() || last_mod.is_some() {
                let sym_id = crate::core::ids::SymbolNodeId(*sym_id_u64);
                if let Some(mut sym) = graph.get_symbol(sym_id)? {
                    sym.first_seen_rev = first;
                    sym.last_modified_rev = last_mod;
                    graph.upsert_symbol(sym)?;
                }
            }
        }
    }

    Ok(())
}

/// Parse file content and produce a map of (qualified_name, kind) -> body_hash.
fn parse_symbols_for_hashes(file_path: &str, content: &[u8]) -> HashMap<SymbolKey, String> {
    let path = Path::new(file_path);
    let output = match parse_file(path, content) {
        Ok(Some(o)) => o,
        _ => return HashMap::new(),
    };

    output
        .symbols
        .into_iter()
        .map(|s| {
            let key = (s.qualified_name, s.kind.as_str().to_string());
            (key, s.body_hash)
        })
        .collect()
}

#[cfg(test)]
mod tests;
