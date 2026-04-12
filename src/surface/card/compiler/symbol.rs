use std::path::PathBuf;
use std::sync::Arc;

use crate::{
    core::ids::{NodeId, SymbolNodeId},
    overlay::{CommentaryEntry, FreshnessState, OverlayStore},
    pipeline::synthesis::CommentaryGenerator,
    structure::graph::GraphStore,
    surface::card::types::{Freshness, OverlayCommentary},
};

use super::io::read_symbol_body;
use super::{Budget, SourceStore, SymbolCard, SymbolRef};

/// Inputs shared across symbol-card construction: graph, repo root, and the
/// optional overlay/generator pair.
pub(super) struct SymbolCardContext<'a> {
    pub graph: &'a dyn GraphStore,
    pub repo_root: &'a Option<PathBuf>,
    pub overlay: Option<&'a Arc<parking_lot::Mutex<dyn OverlayStore>>>,
    pub generator: Option<&'a Arc<dyn CommentaryGenerator>>,
}

pub(super) fn symbol_card(
    ctx: SymbolCardContext<'_>,
    id: SymbolNodeId,
    budget: Budget,
) -> crate::Result<SymbolCard> {
    let symbol = ctx
        .graph
        .get_symbol(id)?
        .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("symbol {id} not found")))?;

    let file = ctx
        .graph
        .get_file(symbol.file_id)?
        .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("file for symbol {id} not found")))?;

    let defined_at = format!("{}:{}", file.path, symbol.body_byte_range.0);

    // Phase 1: edges are file→symbol, not symbol→symbol.
    // Empty until symbol→symbol Calls edges land in stage 5.
    let callers: Vec<SymbolRef> = vec![];
    let callees: Vec<SymbolRef> = vec![];

    // Source body: only for Deep budget.
    let source_body = if budget == Budget::Deep {
        read_symbol_body(ctx.repo_root, &file.path, symbol.body_byte_range)
    } else {
        None
    };

    // Doc comment suppressed for Tiny budget; populated for Normal/Deep if extracted.
    let doc_comment = match budget {
        Budget::Tiny => None,
        _ => symbol.doc_comment.clone(),
    };

    let mut card = SymbolCard {
        symbol: id,
        name: symbol.display_name.clone(),
        qualified_name: symbol.qualified_name.clone(),
        defined_at,
        signature: symbol.signature.clone(),
        doc_comment,
        callers,
        callees,
        tests_touching: vec![],
        last_change: None,
        drift_flag: None,
        source_body,
        approx_tokens: 0,
        source_store: SourceStore::Graph,
        epistemic: symbol.epistemic,
        overlay_commentary: None,
        commentary_state: None,
        proposed_links: None,
        links_state: None,
    };

    // Populate commentary state and links. Budget-withheld at Tiny/Normal; otherwise
    // derived from the overlay store (and optionally the generator) at Deep.
    match budget {
        Budget::Tiny | Budget::Normal => {
            card.commentary_state = Some("budget_withheld".to_string());
            card.links_state = Some("budget_withheld".to_string());
        }
        Budget::Deep => {
            let (text, state) =
                resolve_commentary(&ctx, NodeId::Symbol(id), &file.content_hash, &card)?;
            card.commentary_state = Some(state.as_str().to_string());
            if let Some(text) = text {
                card.overlay_commentary = Some(OverlayCommentary {
                    text,
                    freshness: Freshness::from(state),
                    source_store: SourceStore::Overlay,
                });
            }

            let (links, links_state) = super::links::resolve_proposed_links(
                ctx.overlay.map(|o| &**o),
                ctx.graph,
                NodeId::Symbol(id),
            )?;
            card.proposed_links = links;
            card.links_state = Some(links_state.to_string());
        }
    }

    card.approx_tokens = estimate_tokens_symbol(&card);
    Ok(card)
}

/// Resolve commentary for a Deep-budget card.
///
/// Returns the commentary text (when present) and the observed freshness
/// state. When the overlay store is missing the commentary is `Missing`.
/// When it's present but empty, the generator (if any) is invoked; any
/// returned entry is persisted with the current content hash.
fn resolve_commentary(
    ctx: &SymbolCardContext<'_>,
    node: NodeId,
    current_content_hash: &str,
    card: &SymbolCard,
) -> crate::Result<(Option<String>, FreshnessState)> {
    let overlay = match ctx.overlay {
        Some(overlay) => overlay,
        None => return Ok((None, FreshnessState::Missing)),
    };

    // Fast path: entry already exists.
    if let Some(entry) = overlay.lock().commentary_for(node)? {
        let state = crate::store::overlay::derive_freshness(&entry, current_content_hash);
        return Ok((Some(entry.text), state));
    }

    // No entry yet. Ask the generator, if any.
    let generator = match ctx.generator {
        Some(g) => g,
        None => return Ok((None, FreshnessState::Missing)),
    };

    let prompt = build_generation_context(card);
    match generator.generate(node, &prompt) {
        Ok(Some(mut entry)) => {
            // The generator can't see the current content hash; fill it in
            // here so the persisted entry is immediately `Fresh`.
            entry.provenance.source_content_hash = current_content_hash.to_string();
            persist_and_report(overlay, entry, current_content_hash)
        }
        Ok(None) => Ok((None, FreshnessState::Missing)),
        Err(err) => {
            tracing::warn!(error = %err, "commentary generation failed; reporting missing");
            Ok((None, FreshnessState::Missing))
        }
    }
}

fn persist_and_report(
    overlay: &Arc<parking_lot::Mutex<dyn OverlayStore>>,
    entry: CommentaryEntry,
    current_content_hash: &str,
) -> crate::Result<(Option<String>, FreshnessState)> {
    let text = entry.text.clone();
    let state = crate::store::overlay::derive_freshness(&entry, current_content_hash);
    if let Err(err) = overlay.lock().insert_commentary(entry) {
        tracing::warn!(error = %err, "failed to persist freshly-generated commentary");
        return Ok((Some(text), state));
    }
    Ok((Some(text), state))
}

/// Build the context string passed to the generator. Keeps the payload
/// small: symbol identity, signature, and doc comment.
fn build_generation_context(card: &SymbolCard) -> String {
    let mut s = format!(
        "Symbol: {}\nQualified name: {}\nDefined at: {}\n",
        card.name, card.qualified_name, card.defined_at
    );
    if let Some(sig) = &card.signature {
        s.push_str(&format!("Signature: {sig}\n"));
    }
    if let Some(doc) = &card.doc_comment {
        s.push_str(&format!("Doc comment: {doc}\n"));
    }
    if let Some(body) = &card.source_body {
        s.push_str(&format!("Body:\n{body}\n"));
    }
    s
}

pub(super) fn estimate_tokens_symbol(card: &SymbolCard) -> usize {
    let mut len = card.name.len()
        + card.qualified_name.len()
        + card.defined_at.len()
        + card.signature.as_deref().map_or(0, str::len)
        + card.doc_comment.as_deref().map_or(0, str::len)
        + card.source_body.as_deref().map_or(0, str::len);

    for sym_ref in card.callers.iter().chain(card.callees.iter()) {
        len += sym_ref.qualified_name.len() + sym_ref.location.len();
    }

    if let Some(c) = &card.overlay_commentary {
        len += c.text.len();
    }

    (len / 4).max(10)
}
