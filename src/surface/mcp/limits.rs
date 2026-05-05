//! Shared MCP input limits.

/// Maximum accepted search query length.
pub const MAX_SEARCH_QUERY_CHARS: usize = 512;
/// Maximum targets accepted by a batch card request.
pub const MAX_CARD_TARGETS: usize = 16;
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
