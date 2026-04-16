use serde::{Deserialize, Serialize};

use crate::core::ids::SymbolNodeId;

use super::SourceStore;

/// Classification of an execution entry point.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryPointKind {
    /// Binary entry point: a `main` function in `src/main.rs` or `src/bin/`.
    Binary,
    /// CLI command handler in a file whose path contains `cli`, `command`, or `cmd`.
    CliCommand,
    /// HTTP route handler: name starts with `handle_`, `serve_`, or `route_`,
    /// or the file path contains `handler`, `route`, or `router`.
    HttpHandler,
    /// Public item at a library root (`src/lib.rs` or a `mod.rs` boundary).
    LibRoot,
}

/// A single detected execution entry point.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryPoint {
    /// Stable node ID of the entry-point symbol.
    pub symbol: SymbolNodeId,
    /// Fully qualified name within its file.
    pub qualified_name: String,
    /// File path and byte offset (e.g. `src/main.rs:0`).
    pub location: String,
    /// Classification of this entry point.
    pub kind: EntryPointKind,
    /// Number of unique callers in the graph. `None` at `Tiny` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_count: Option<usize>,
    /// Doc comment truncated to 80 characters. `None` at `Tiny` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    /// One-line signature. `None` below `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// EntryPointCard — answers "where does execution start in this scope?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryPointCard {
    /// Optional path-prefix scope that was requested (`None` = whole repo).
    pub scope: Option<String>,
    /// Detected entry points, sorted by kind then file path, capped at 20.
    pub entry_points: Vec<EntryPoint>,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Source store (always `Graph` for entry-point cards).
    pub source_store: SourceStore,
}
