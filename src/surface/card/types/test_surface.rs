use serde::{Deserialize, Serialize};

use crate::core::ids::SymbolNodeId;

use super::SourceStore;

/// How a test was associated with a source file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestAssociation {
    /// Matched by SymbolKind::Test.
    SymbolKind,
    /// Matched by file path convention.
    PathConvention,
    /// Matched by both signals.
    Both,
}

/// A single test entry discovered for a source file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestEntry {
    /// Node ID of the test symbol.
    pub symbol_id: SymbolNodeId,
    /// Fully qualified name of the test.
    pub qualified_name: String,
    /// Path of the file containing this test (repo-relative).
    pub file_path: String,
    /// The associated source file path.
    pub source_file: String,
    /// How this test was associated with the source file.
    pub association: TestAssociation,
    /// One-line signature. Populated only at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Doc comment, truncated to 120 chars. Populated only at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    /// Production symbols called by this test. Populated only at `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub covers: Option<Vec<SymbolNodeId>>,
}

/// TestSurfaceCard — answers "what tests cover this code?"
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestSurfaceCard {
    /// The scope this card was compiled for (file path or directory).
    pub scope: String,
    /// Discovered test entries grouped by source file.
    pub tests: Vec<TestEntry>,
    /// Total count of test files discovered.
    pub test_file_count: usize,
    /// Total count of test symbols discovered.
    pub test_symbol_count: usize,
    /// Approximate token count of this card.
    pub approx_tokens: usize,
    /// Source store (always `Graph` for test-surface cards).
    pub source_store: SourceStore,
}
