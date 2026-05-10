//! Operational mode enum for synrepo.

use serde::{Deserialize, Serialize};

/// Which operational mode synrepo runs in.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Bootstrap defaults here when repository inspection does not find
    /// rationale markdown under the configured concept directories.
    /// Explain output is optional and explicitly requested; when generated,
    /// it writes to the overlay. Concept nodes are disabled unless
    /// human-authored concept directories exist.
    #[default]
    Auto,
    /// Bootstrap recommends or selects this when repository inspection
    /// finds rationale markdown under the configured concept directories,
    /// unless an explicit or already-configured mode is kept instead.
    /// Explain proposals go to a review queue. Concept nodes are
    /// enabled when human-authored ADR directories exist.
    Curated,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Auto => f.write_str("auto"),
            Mode::Curated => f.write_str("curated"),
        }
    }
}
