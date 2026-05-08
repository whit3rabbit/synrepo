use serde_json::{Map, Value};

use crate::structure::graph::ConceptNode;

use super::truncate_chars;

pub(crate) fn concept_summary_card(concept: &ConceptNode) -> Value {
    let mut map = Map::new();
    map.insert("kind".to_string(), Value::String("concept".to_string()));
    map.insert("title".to_string(), Value::String(concept.title.clone()));
    map.insert("path".to_string(), Value::String(concept.path.clone()));
    map.insert(
        "source_store".to_string(),
        Value::String("graph".to_string()),
    );

    if !concept.aliases.is_empty() {
        map.insert(
            "aliases".to_string(),
            Value::Array(
                concept
                    .aliases
                    .iter()
                    .map(|alias| Value::String(alias.clone()))
                    .collect(),
            ),
        );
    }
    if let Some(status) = &concept.status {
        map.insert("status".to_string(), Value::String(status.clone()));
    }
    if let Some(summary) = &concept.summary {
        map.insert(
            "summary".to_string(),
            Value::String(truncate_chars(summary, 240)),
        );
    }

    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::ConceptNodeId;
    use crate::core::provenance::Provenance;
    use crate::structure::graph::Epistemic;

    #[test]
    fn concept_summary_omits_decision_body_and_empty_aliases() {
        let concept = ConceptNode {
            id: ConceptNodeId(1),
            path: "docs/adr/0001.md".to_string(),
            title: "Keep Context Small".to_string(),
            aliases: Vec::new(),
            summary: Some("Use routing packets by default.".to_string()),
            status: None,
            decision_body: Some("Large decision body that should not be serialized.".to_string()),
            last_observed_rev: Some(1),
            epistemic: Epistemic::HumanDeclared,
            provenance: Provenance::structural("test", "rev", Vec::new()),
        };

        let value = concept_summary_card(&concept);
        assert_eq!(value["kind"], "concept");
        assert_eq!(value["title"], "Keep Context Small");
        assert!(value.get("decision_body").is_none());
        assert!(value.get("aliases").is_none());
        assert!(value.get("last_observed_rev").is_none());
        assert!(value.get("provenance").is_none());
    }
}
