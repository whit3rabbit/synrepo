//! Git-derived intelligence types for cards.

mod projection;
mod types;

pub(crate) use projection::symbol_last_change_from_insights;
pub use types::{
    FileGitCoChange, FileGitCommit, FileGitIntelligence, FileGitOwnership, LastChangeGranularity,
    SymbolLastChange,
};
pub(crate) use types::FILE_NODE_GIT_INSIGHT_LIMIT;
