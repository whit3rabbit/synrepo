//! Error types for synrepo.

use thiserror::Error;

/// synrepo's top-level error type.
#[derive(Debug, Error)]
pub enum Error {
    /// I/O failure reading or writing a file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// sqlite error from the graph store or overlay store.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Tree-sitter parse failure or query error.
    #[error("parse error in {path}: {message}")]
    Parse {
        /// Path of the file that failed to parse.
        path: String,
        /// Human-readable message from the parser.
        message: String,
    },

    /// A file was skipped because its encoding could not be sniffed.
    #[error("unsupported encoding in {path}")]
    UnsupportedEncoding {
        /// Path of the file that was skipped.
        path: String,
    },

    /// Git operation failed (worktree, shallow clone, rev-parse).
    #[error("git error: {0}")]
    Git(String),

    /// Config file parse failure.
    #[error("config error: {0}")]
    Config(String),

    /// A node ID referenced an item that does not exist in the graph.
    #[error("node not found: {0}")]
    NodeNotFound(crate::core::ids::NodeId),

    /// Identity resolution failed to find a stable home for a file or symbol.
    #[error("identity resolution failed: {0}")]
    IdentityResolution(String),

    /// Catch-all for errors that don't fit another variant.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// synrepo's Result type.
pub type Result<T> = std::result::Result<T, Error>;