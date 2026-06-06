use std::fmt;

use serde::{de, Deserialize, Deserializer};

use super::types::GroundingMode;

impl<'de> Deserialize<'de> for GroundingMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(GroundingModeVisitor)
    }
}

struct GroundingModeVisitor;

impl<'de> de::Visitor<'de> for GroundingModeVisitor {
    type Value = GroundingMode;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("one of `required`, `preferred`, or `off`")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match value {
            "required" => Ok(GroundingMode::Required),
            "preferred" => Ok(GroundingMode::Preferred),
            "off" => Ok(GroundingMode::Off),
            "observed" => Err(E::custom(
                "invalid grounding mode `observed`: `observed` is an evidence confidence label, not a grounding mode; use `required`, `preferred`, or `off`",
            )),
            other => Err(E::custom(format!(
                "invalid grounding mode `{other}`: expected `required`, `preferred`, or `off`; do not use evidence confidence labels such as `observed` as modes"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::surface::context::ContextAskRequest;

    #[test]
    fn grounding_structured_modes_accept_only_public_values() {
        for (raw, expected) in [
            ("required", GroundingMode::Required),
            ("preferred", GroundingMode::Preferred),
            ("off", GroundingMode::Off),
        ] {
            let request: ContextAskRequest = serde_json::from_value(json!({
                "ask": "review module",
                "ground": { "mode": raw }
            }))
            .unwrap();

            assert_eq!(request.ground.mode, expected);
        }
    }

    #[test]
    fn grounding_citations_alias_uses_same_mode_values() {
        let request: ContextAskRequest = serde_json::from_value(json!({
            "ask": "review module",
            "ground": { "citations": "preferred" }
        }))
        .unwrap();

        assert_eq!(request.ground.mode, GroundingMode::Preferred);
    }

    #[test]
    fn grounding_rejects_observed_as_structured_mode() {
        for request in [
            json!({
                "ask": "review module",
                "ground": { "mode": "observed" }
            }),
            json!({
                "ask": "review module",
                "ground": { "citations": "observed" }
            }),
        ] {
            let error = serde_json::from_value::<ContextAskRequest>(request)
                .unwrap_err()
                .to_string();

            assert!(error.contains("invalid grounding mode `observed`"));
            assert!(error.contains("evidence confidence label"));
            assert!(error.contains("`required`, `preferred`, or `off`"));
        }
    }

    #[test]
    fn grounding_string_shorthand_still_accepts_observed_source_phrase() {
        let request: ContextAskRequest = serde_json::from_value(json!({
            "ask": "review module",
            "ground": "observed graph/source only"
        }))
        .unwrap();

        assert_eq!(request.ground.mode, GroundingMode::Required);
        assert!(request.ground.include_spans);
        assert!(!request.ground.allow_overlay);
    }
}
