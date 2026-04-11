//! Epistemic origin type for canonical graph rows.
//!
//! Hard invariant: this enum must only contain variants for directly-observed
//! provenance. Machine-authored content lives in the overlay layer and uses
//! [`crate::overlay::OverlayEpistemic`] instead. The type boundary is
//! enforced by the type system — do not add machine variants here.

use serde::{Deserialize, Serialize};

/// Epistemic origin of a graph row.
///
/// The canonical graph only holds `parser_observed`, `human_declared`, and
/// `git_observed` rows. Machine-authored content lives in the overlay and
/// uses [`crate::overlay::OverlayEpistemic`] instead. This enum does not
/// include the machine variants on purpose — the type system enforces the
/// graph/overlay boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Epistemic {
    /// Directly observed by tree-sitter or the markdown parser.
    ParserObserved,
    /// Present in a human-authored source (frontmatter, inline marker, ADR).
    HumanDeclared,
    /// Derived from git history (rename, co-change, ownership, blame).
    GitObserved,
}
