//! Handoffs command implementation.

use std::path::Path;

use synrepo::surface::handoffs::HandoffsRequest;
use synrepo::surface::handoffs::{collect_handoffs, to_json as handoffs_to_json, to_markdown};

/// Run the handoffs command.
pub(crate) fn handoffs(
    repo_root: &Path,
    limit: Option<usize>,
    since: Option<u32>,
    json: bool,
) -> anyhow::Result<()> {
    let request = HandoffsRequest {
        limit: limit.unwrap_or(20),
        since_days: since.unwrap_or(30),
    };

    let items = collect_handoffs(repo_root, &request)?;

    if json {
        println!("{}", handoffs_to_json(&items));
    } else {
        println!("{}", to_markdown(&items));
    }

    Ok(())
}
