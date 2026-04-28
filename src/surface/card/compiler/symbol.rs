use std::path::PathBuf;
use std::sync::Arc;

use crate::{
    core::ids::{NodeId, SymbolNodeId},
    overlay::{FreshnessState, OverlayStore},
    structure::graph::{Edge, EdgeKind, GraphReader},
    surface::card::accounting::{raw_file_token_estimate, ContextAccounting},
    surface::card::git::symbol_last_change_from_insights,
    surface::card::types::{Freshness, OverlayCommentary},
};

use super::io::read_symbol_body;
use super::{Budget, GraphCardCompiler, SourceStore, SymbolCard, SymbolRef};

/// Inputs shared across symbol-card construction: graph, repo root, and the
/// optional overlay/generator pair.
pub(super) struct SymbolCardContext<'a> {
    pub compiler: &'a GraphCardCompiler,
    pub graph: &'a dyn GraphReader,
    pub repo_root: &'a Option<PathBuf>,
    pub overlay: Option<&'a Arc<parking_lot::Mutex<dyn OverlayStore>>>,
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

    let (callers, callees) = if budget == Budget::Deep {
        (
            symbol_refs_from_edges(
                ctx.graph
                    .inbound(NodeId::Symbol(id), Some(EdgeKind::Calls))?,
                |edge| edge.from,
                ctx.graph,
            )?,
            symbol_refs_from_edges(
                ctx.graph
                    .outbound(NodeId::Symbol(id), Some(EdgeKind::Calls))?,
                |edge| edge.to,
                ctx.graph,
            )?,
        )
    } else {
        (Vec::new(), Vec::new())
    };

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

    let last_change = if budget == Budget::Tiny {
        None
    } else {
        let include_summary = budget == Budget::Deep;
        let rev = symbol.last_modified_rev.as_deref();
        ctx.compiler
            .resolve_file_git_intelligence(&file.path)
            .and_then(|arc| symbol_last_change_from_insights(&arc, include_summary, rev))
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
        last_change,
        drift_flag: None,
        source_body,
        approx_tokens: 0,
        context_accounting: ContextAccounting::placeholder(budget),
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
    card.context_accounting = ContextAccounting::new(
        budget,
        card.approx_tokens,
        raw_file_token_estimate(ctx.repo_root.as_deref(), &file.path),
        vec![file.content_hash],
    );
    Ok(card)
}

fn symbol_refs_from_edges(
    edges: Vec<Edge>,
    endpoint: impl Fn(&Edge) -> NodeId,
    graph: &dyn GraphReader,
) -> crate::Result<Vec<SymbolRef>> {
    let mut refs = Vec::new();
    for edge in edges {
        let NodeId::Symbol(symbol_id) = endpoint(&edge) else {
            continue;
        };
        let Some(symbol) = graph.get_symbol(symbol_id)? else {
            continue;
        };
        let Some(file) = graph.get_file(symbol.file_id)? else {
            continue;
        };
        refs.push(SymbolRef {
            id: symbol_id,
            qualified_name: symbol.qualified_name,
            location: format!("{}:{}", file.path, symbol.body_byte_range.0),
        });
    }
    Ok(refs)
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
    _card: &SymbolCard,
) -> crate::Result<(Option<String>, FreshnessState)> {
    let overlay = match ctx.overlay {
        Some(overlay) => overlay,
        None => return Ok((None, FreshnessState::Missing)),
    };

    // Card reads are strictly read-only: return existing entry if found,
    // otherwise report missing.
    if let Some(entry) = overlay.lock().commentary_for(node)? {
        let state = crate::store::overlay::derive_freshness(&entry, current_content_hash);
        return Ok((Some(entry.text), state));
    }

    Ok((None, FreshnessState::Missing))
}

/// Build the context string passed to the generator. Keeps the payload
/// small: symbol identity, signature, and doc comment.
pub(crate) fn build_generation_context(card: &SymbolCard) -> String {
    let target = format!(
        "Target node: {}\nTarget kind: symbol\nSymbol: {}\nQualified name: {}\nDefined at: {}\n",
        card.symbol, card.name, card.qualified_name, card.defined_at
    );
    let mut s = String::new();
    if let Some(sig) = &card.signature {
        s.push_str(&format!("Signature: {sig}\n"));
    }
    if let Some(doc) = &card.doc_comment {
        s.push_str(&format!("<doc_comment>\n{doc}\n</doc_comment>\n"));
    }
    if let Some(body) = &card.source_body {
        s.push_str(&format!("<source_code>\n{body}\n</source_code>\n"));
    }
    if card.tests_touching.is_empty() {
        s.push_str("<associated_tests>\nnone found in graph card\n</associated_tests>\n");
    } else {
        s.push_str("<associated_tests>\n");
        for test in &card.tests_touching {
            s.push_str(&format!("{} at {}\n", test.qualified_name, test.location));
        }
        s.push_str("</associated_tests>\n");
    }
    crate::pipeline::explain::commentary_template::build_commentary_context(&target, &s)
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
