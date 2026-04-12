//! Cards: the user-facing unit of compressed context.
//!
//! Cards are small, structured, deterministic records compiled from the
//! graph and source code. They are not prose summaries. See
//! `synrepo-design-v4.md` section "Cards and the context budget protocol"
//! for the full rationale.

use serde::{Deserialize, Serialize};

use crate::core::ids::{FileNodeId, NodeId, SymbolNodeId};

pub mod compiler;
pub mod decision;
mod git;
mod types;

pub use decision::DecisionCard;
pub use git::{FileGitCoChange, FileGitCommit, FileGitIntelligence, FileGitOwnership};
pub use types::{
    FileCard, FileRef, Freshness, ModuleCard, OverlayCommentary, SymbolCard, SymbolRef,
};

/// Context budget tier for a card request.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Budget {
    /// Roughly 200 tokens per card, ~1k tokens total.
    /// Card headers only: name, signature, location, top 3 callers/callees,
    /// drift flag. The default for orientation and routing.
    #[default]
    Tiny,
    /// Roughly 500 tokens per card, ~3k tokens total.
    /// Full card including test surface and recent change context.
    Normal,
    /// Roughly 2k tokens per card, ~10k tokens total.
    /// Full card plus actual source body, plus linked DecisionCards if available.
    /// Only for when the agent is about to write code that depends on the exact source.
    Deep,
}

impl Budget {
    /// Approximate per-card token budget for this tier.
    pub fn per_card_tokens(self) -> usize {
        match self {
            Budget::Tiny => 200,
            Budget::Normal => 500,
            Budget::Deep => 2000,
        }
    }

    /// Approximate total token budget for a response at this tier.
    pub fn total_budget_tokens(self) -> usize {
        match self {
            Budget::Tiny => 1_000,
            Budget::Normal => 3_000,
            Budget::Deep => 10_000,
        }
    }
}

/// Which store a field in a card response came from.
///
/// Every field in every card response is tagged with this so the agent
/// can reason about what it trusts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStore {
    /// From the canonical graph (parser_observed, human_declared, git_observed).
    Graph,
    /// From the overlay (machine_authored_*). Not present in phase 0/1 cards.
    Overlay,
}

/// Trait for compiling cards from the graph store.
pub trait CardCompiler {
    /// Compile a SymbolCard at the given budget.
    fn symbol_card(&self, id: SymbolNodeId, budget: Budget) -> crate::Result<SymbolCard>;

    /// Compile a FileCard at the given budget.
    fn file_card(&self, id: FileNodeId, budget: Budget) -> crate::Result<FileCard>;

    /// Resolve a human-readable target string (a path, a qualified name,
    /// or a symbol name) to a NodeId for card compilation.
    fn resolve_target(&self, target: &str) -> crate::Result<Option<NodeId>>;
}
