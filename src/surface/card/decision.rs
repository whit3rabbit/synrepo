//! `DecisionCard` — optional rationale output backed by human-authored ConceptNodes.
//!
//! Returned only when the queried node has incoming `Governs` edges from ConceptNodes.
//! Never overrides structural card truth; it surfaces design intent alongside facts.

use serde::{Deserialize, Serialize};

use crate::core::ids::NodeId;

use super::{Budget, Freshness};

/// A card representing a human-authored design decision linked to a graph node.
///
/// Built from a `ConceptNode` that has outgoing `Governs` edges to the queried
/// node. Absent when no governing concepts exist (never null, never empty shell).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionCard {
    /// Title of the ADR or pattern document.
    pub title: String,
    /// Decision status from frontmatter (e.g. "Accepted", "Deprecated").
    pub status: Option<String>,
    /// Body text of the decision section.
    pub decision_body: Option<String>,
    /// All node IDs governed by this decision (outgoing Governs edges from the concept).
    pub governed_node_ids: Vec<NodeId>,
    /// Repo-relative path of the source markdown document.
    pub source_path: String,
    /// Freshness of the source document relative to the last structural compile.
    pub freshness: Freshness,
}

impl DecisionCard {
    /// Render the card as a JSON value at the given budget tier.
    ///
    /// - Tiny (compact): title and governed_node_ids only.
    /// - Normal (standard): adds status and decision_body truncated to 300 chars.
    /// - Deep (full): all fields.
    pub fn render(&self, budget: Budget) -> serde_json::Value {
        let governed_ids: Vec<String> = self
            .governed_node_ids
            .iter()
            .map(|id| id.to_string())
            .collect();
        match budget {
            Budget::Tiny => serde_json::json!({
                "title": self.title,
                "governed_node_ids": governed_ids,
            }),
            Budget::Normal => serde_json::json!({
                "title": self.title,
                "status": self.status,
                "decision_body": self.decision_body.as_deref().map(|s| truncate(s, 300)),
                "governed_node_ids": governed_ids,
            }),
            Budget::Deep => serde_json::json!({
                "title": self.title,
                "status": self.status,
                "decision_body": self.decision_body,
                "governed_node_ids": governed_ids,
                "source_path": self.source_path,
                "freshness": self.freshness,
            }),
        }
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        None => s.to_string(),
        Some((end, _)) => format!("{}…", &s[..end]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    fn sample_card() -> DecisionCard {
        DecisionCard {
            title: "Use SQLite for Graph Storage".to_string(),
            status: Some("Accepted".to_string()),
            decision_body: Some(
                "We store the canonical graph in SQLite because it provides \
                 transactional writes and zero-dependency deployment without \
                 a separate server process."
                    .to_string(),
            ),
            governed_node_ids: vec![],
            source_path: "docs/adr/0001-sqlite.md".to_string(),
            freshness: Freshness::Fresh,
        }
    }

    #[test]
    fn decision_card_render_tiny() {
        let rendered = serde_json::to_string_pretty(&sample_card().render(Budget::Tiny)).unwrap();
        assert_snapshot!("decision_card_tiny", rendered);
    }

    #[test]
    fn decision_card_render_normal() {
        let rendered = serde_json::to_string_pretty(&sample_card().render(Budget::Normal)).unwrap();
        assert_snapshot!("decision_card_normal", rendered);
    }

    #[test]
    fn decision_card_render_deep() {
        let rendered = serde_json::to_string_pretty(&sample_card().render(Budget::Deep)).unwrap();
        assert_snapshot!("decision_card_deep", rendered);
    }

    #[test]
    fn decision_card_normal_truncates_body_at_300_chars() {
        let long_body = "x".repeat(400);
        let card = DecisionCard {
            title: "T".to_string(),
            status: None,
            decision_body: Some(long_body),
            governed_node_ids: vec![],
            source_path: "docs/adr/0001.md".to_string(),
            freshness: Freshness::Fresh,
        };
        let rendered = card.render(Budget::Normal);
        let body = rendered["decision_body"].as_str().unwrap();
        // truncated to 300 chars + ellipsis
        assert!(
            body.chars().count() <= 302,
            "truncated body must be ≤302 chars"
        );
        assert!(body.ends_with('…'));
    }
}
