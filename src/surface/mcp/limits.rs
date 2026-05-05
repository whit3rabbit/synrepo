//! Shared MCP input limits.

/// Default response token target for MCP tools.
pub const DEFAULT_RESPONSE_TOKEN_CAP: usize = 4_000;
/// Hard maximum response token cap for MCP tools.
pub const MAX_RESPONSE_TOKEN_CAP: usize = 12_000;
/// Conservative bytes-per-token estimate used by response caps.
pub const BYTES_PER_TOKEN_ESTIMATE: usize = 3;
/// Default lexical search result limit.
pub const DEFAULT_SEARCH_LIMIT: usize = 10;
/// Maximum lexical search result limit.
pub const MAX_SEARCH_LIMIT: usize = 50;
/// Default compact search token budget.
pub const DEFAULT_SEARCH_BUDGET_TOKENS: usize = 1_500;
/// Search cards mode requires narrow result sets.
pub const MAX_SEARCH_CARDS_LIMIT: usize = 5;
/// Default context-pack target limit.
pub const DEFAULT_CONTEXT_PACK_LIMIT: usize = 5;
/// Maximum context-pack target count.
pub const MAX_CONTEXT_PACK_TARGETS: usize = 10;
/// Default context-pack token cap.
pub const DEFAULT_CONTEXT_PACK_TOKEN_CAP: usize = 6_000;
/// Default advisory notes list limit.
pub const DEFAULT_NOTES_LIMIT: usize = 10;
/// Maximum advisory notes list limit.
pub const MAX_NOTES_LIMIT: usize = 50;
/// Default findings list limit.
pub const DEFAULT_FINDINGS_LIMIT: usize = 25;
/// Maximum findings list limit.
pub const MAX_FINDINGS_LIMIT: usize = 50;
/// Default graph primitive edge limit.
pub const DEFAULT_GRAPH_LIMIT: usize = 100;
/// Maximum graph primitive edge limit.
pub const MAX_GRAPH_LIMIT: usize = 500;
/// Maximum accepted search query length.
pub const MAX_SEARCH_QUERY_CHARS: usize = 512;
/// Maximum targets accepted by a batch card request.
pub const MAX_CARD_TARGETS: usize = 10;
/// Maximum targets accepted by a deep batch card request.
pub const MAX_DEEP_CARD_TARGETS: usize = 3;
/// Maximum advisory note claim length.
pub const MAX_NOTE_CLAIM_CHARS: usize = 4_000;
/// Maximum evidence objects accepted with one advisory note.
pub const MAX_NOTE_EVIDENCE: usize = 32;
/// Maximum source hashes accepted with one advisory note.
pub const MAX_NOTE_SOURCE_HASHES: usize = 32;

/// Validate a string field against a character limit.
pub fn check_chars(field: &str, value: &str, max_chars: usize) -> anyhow::Result<()> {
    let count = value.chars().count();
    if count > max_chars {
        anyhow::bail!("{field} exceeds {max_chars} characters");
    }
    Ok(())
}

/// Validate a collection length against a limit.
pub fn check_len(field: &str, len: usize, max_len: usize) -> anyhow::Result<()> {
    if len > max_len {
        anyhow::bail!("{field} has {len} entries, exceeding limit {max_len}");
    }
    Ok(())
}

/// Clamp a requested MCP list limit to the safe server-side range.
pub fn bounded_limit(requested: Option<usize>, default: usize, max: usize) -> usize {
    requested.unwrap_or(default).clamp(1, max)
}

/// Clamp an already-defaulted MCP list limit to the safe server-side range.
pub fn bounded_limit_value(requested: usize, _default: usize, max: usize) -> usize {
    requested.clamp(1, max)
}
