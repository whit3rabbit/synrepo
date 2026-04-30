//! Serialization helpers: row → `OverlayLink` reconstruction, link validation,
//! and enum ↔ snake_case string mappings shared by read and write paths.

use std::str::FromStr;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::overlay::{
    CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic, OverlayLink,
};

pub(super) fn row_to_overlay_link(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<crate::Result<OverlayLink>> {
    Ok((|| -> crate::Result<OverlayLink> {
        let from_node: String = row.get(0)?;
        let to_node: String = row.get(1)?;
        let kind: String = row.get(2)?;
        let epistemic: String = row.get(3)?;
        let source_spans_json: String = row.get(4)?;
        let target_spans_json: String = row.get(5)?;
        let from_content_hash: String = row.get(6)?;
        let to_content_hash: String = row.get(7)?;
        let confidence_score: f32 = row.get(8)?;
        let confidence_tier: String = row.get(9)?;
        let rationale: Option<String> = row.get(10)?;
        let pass_id: String = row.get(11)?;
        let model_identity: String = row.get(12)?;
        let generated_at: String = row.get(13)?;

        let from = NodeId::from_str(&from_node)
            .map_err(|e| anyhow::anyhow!("stored from_node invalid: {e}"))?;
        let to = NodeId::from_str(&to_node)
            .map_err(|e| anyhow::anyhow!("stored to_node invalid: {e}"))?;
        let kind = parse_overlay_edge_kind(&kind)?;
        let epistemic = parse_overlay_epistemic(&epistemic)?;
        let tier = parse_confidence_tier(&confidence_tier)?;
        let source_spans: Vec<CitedSpan> =
            serde_json::from_str(&source_spans_json).map_err(anyhow_err)?;
        let target_spans: Vec<CitedSpan> =
            serde_json::from_str(&target_spans_json).map_err(anyhow_err)?;
        let generated_at = OffsetDateTime::parse(&generated_at, &Rfc3339)
            .map_err(|e| anyhow::anyhow!("invalid generated_at: {e}"))?;

        Ok(OverlayLink {
            from,
            to,
            kind,
            epistemic,
            source_spans,
            target_spans,
            from_content_hash,
            to_content_hash,
            confidence_score,
            confidence_tier: tier,
            rationale,
            provenance: CrossLinkProvenance {
                pass_id,
                model_identity,
                generated_at,
            },
        })
    })())
}

pub(super) fn validate_link(link: &OverlayLink) -> crate::Result<()> {
    if !link.has_complete_provenance() {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "cross-link candidate is missing required provenance fields"
        )));
    }
    if link.source_spans.is_empty() || link.target_spans.is_empty() {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "cross-link candidate must carry at least one source and one target span"
        )));
    }
    if link.from_content_hash.is_empty() || link.to_content_hash.is_empty() {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "cross-link candidate must carry both endpoint content hashes"
        )));
    }
    if link.from == link.to {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "cross-link candidate from/to must be distinct nodes"
        )));
    }
    Ok(())
}

pub(super) fn overlay_edge_kind_as_str(k: OverlayEdgeKind) -> &'static str {
    match k {
        OverlayEdgeKind::References => "references",
        OverlayEdgeKind::Governs => "governs",
        OverlayEdgeKind::DerivedFrom => "derived_from",
        OverlayEdgeKind::Mentions => "mentions",
    }
}

pub(super) fn parse_overlay_edge_kind(s: &str) -> crate::Result<OverlayEdgeKind> {
    match s {
        "references" => Ok(OverlayEdgeKind::References),
        "governs" => Ok(OverlayEdgeKind::Governs),
        "derived_from" => Ok(OverlayEdgeKind::DerivedFrom),
        "mentions" => Ok(OverlayEdgeKind::Mentions),
        other => Err(crate::Error::Other(anyhow::anyhow!(
            "invalid overlay edge kind: {other}"
        ))),
    }
}

pub(super) fn overlay_epistemic_as_str(e: OverlayEpistemic) -> &'static str {
    match e {
        OverlayEpistemic::MachineAuthoredHighConf => "machine_authored_high_conf",
        OverlayEpistemic::MachineAuthoredLowConf => "machine_authored_low_conf",
    }
}

pub(super) fn parse_overlay_epistemic(s: &str) -> crate::Result<OverlayEpistemic> {
    match s {
        "machine_authored_high_conf" => Ok(OverlayEpistemic::MachineAuthoredHighConf),
        "machine_authored_low_conf" => Ok(OverlayEpistemic::MachineAuthoredLowConf),
        other => Err(crate::Error::Other(anyhow::anyhow!(
            "invalid overlay epistemic: {other}"
        ))),
    }
}

pub(super) fn parse_confidence_tier(s: &str) -> crate::Result<ConfidenceTier> {
    match s {
        "high" => Ok(ConfidenceTier::High),
        "review_queue" => Ok(ConfidenceTier::ReviewQueue),
        "below_threshold" => Ok(ConfidenceTier::BelowThreshold),
        other => Err(crate::Error::Other(anyhow::anyhow!(
            "invalid confidence tier: {other}"
        ))),
    }
}

pub(super) fn anyhow_err<E: std::fmt::Display>(e: E) -> crate::Error {
    crate::Error::Other(anyhow::anyhow!("{e}"))
}
