use serde_json::Value;

use crate::surface::context::{CitedLineSpan, Confidence, ContextEvidence, ContextSourceRef};

const MAX_EVIDENCE_ITEMS: usize = 20;

pub(super) fn collect_evidence(packet: &Value, include_spans: bool) -> Vec<ContextEvidence> {
    let mut evidence = Vec::new();
    let Some(artifacts) = packet.get("artifacts").and_then(Value::as_array) else {
        return evidence;
    };
    for artifact in artifacts {
        if evidence.len() >= MAX_EVIDENCE_ITEMS {
            break;
        }
        if artifact.get("status").and_then(Value::as_str) != Some("ok") {
            continue;
        }
        match artifact.get("artifact_type").and_then(Value::as_str) {
            Some("search") => collect_search_evidence(artifact, include_spans, &mut evidence),
            _ => collect_artifact_evidence(artifact, include_spans, &mut evidence),
        }
    }
    evidence.truncate(MAX_EVIDENCE_ITEMS);
    evidence
}

fn collect_artifact_evidence(
    artifact: &Value,
    include_spans: bool,
    evidence: &mut Vec<ContextEvidence>,
) {
    let content = artifact.get("content").unwrap_or(&Value::Null);
    let (source, line) = source_for_artifact(artifact, content);
    let source_store = content
        .get("source_store")
        .and_then(Value::as_str)
        .unwrap_or("graph");
    let claim = format!(
        "Included {} for {}",
        artifact
            .get("artifact_type")
            .and_then(Value::as_str)
            .unwrap_or("artifact"),
        artifact
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("unknown target")
    );
    evidence.push(evidence_item(
        claim,
        source,
        line,
        source_store,
        source_hash_for(content),
        include_spans,
    ));
}

fn collect_search_evidence(
    artifact: &Value,
    include_spans: bool,
    evidence: &mut Vec<ContextEvidence>,
) {
    let content = artifact.get("content").unwrap_or(&Value::Null);
    let query = content
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("search query");
    let source_store = content
        .get("source_store")
        .and_then(Value::as_str)
        .unwrap_or("substrate_index");
    if let Some(rows) = content.get("results").and_then(Value::as_array) {
        for row in rows.iter().take(3) {
            push_search_row_evidence(query, row, source_store, include_spans, evidence);
        }
        return;
    }
    let Some(groups) = content.get("file_groups").and_then(Value::as_array) else {
        return;
    };
    for group in groups.iter().take(3) {
        let source = group
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("unknown source");
        let line = group
            .get("lines")
            .and_then(Value::as_array)
            .and_then(|lines| lines.first())
            .and_then(|line| line.get("line"))
            .and_then(Value::as_u64);
        evidence.push(evidence_item(
            format!("Search for `{query}` matched {source}"),
            source.to_string(),
            line,
            source_store,
            None,
            include_spans,
        ));
    }
}

fn push_search_row_evidence(
    query: &str,
    row: &Value,
    source_store: &str,
    include_spans: bool,
    evidence: &mut Vec<ContextEvidence>,
) {
    let Some(source) = row.get("path").and_then(Value::as_str) else {
        return;
    };
    evidence.push(evidence_item(
        format!("Search for `{query}` matched {source}"),
        source.to_string(),
        row.get("line").and_then(Value::as_u64),
        source_store,
        None,
        include_spans,
    ));
}

fn evidence_item(
    claim: String,
    source: String,
    line: Option<u64>,
    source_store: &str,
    content_hash: Option<String>,
    include_spans: bool,
) -> ContextEvidence {
    let span = if include_spans {
        line.map(|line| CitedLineSpan {
            start_line: line,
            end_line: line,
        })
    } else {
        None
    };
    let spans = span.iter().cloned().collect();
    ContextEvidence {
        claim,
        source: source.clone(),
        span,
        spans,
        source_store: source_store.to_string(),
        confidence: confidence_for_source_store(source_store),
        provenance: vec![ContextSourceRef {
            path: source,
            source_store: source_store.to_string(),
            content_hash,
        }],
    }
}

fn confidence_for_source_store(source_store: &str) -> Confidence {
    if source_store == "overlay" {
        Confidence::MachineOverlayHigh
    } else {
        Confidence::Observed
    }
}

fn source_for_artifact(artifact: &Value, content: &Value) -> (String, Option<u64>) {
    if let Some(path) = content.get("path").and_then(Value::as_str) {
        return (path.to_string(), None);
    }
    if let Some(source_path) = content.get("source_path").and_then(Value::as_str) {
        return (source_path.to_string(), None);
    }
    if let Some(defined_at) = content.get("defined_at").and_then(Value::as_str) {
        if let Some((path, _offset_or_line)) = defined_at.rsplit_once(':') {
            return (path.to_string(), None);
        }
    }
    (
        artifact
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("unknown source")
            .to_string(),
        None,
    )
}

fn source_hash_for(content: &Value) -> Option<String> {
    content
        .get("context_accounting")
        .and_then(|accounting| accounting.get("source_hashes"))
        .and_then(Value::as_array)
        .and_then(|hashes| hashes.first())
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::collect_evidence;

    #[test]
    fn search_evidence_uses_artifact_source_store() {
        let packet = json!({
            "artifacts": [{
                "status": "ok",
                "artifact_type": "search",
                "content": {
                    "query": "alpha",
                    "source_store": "substrate_index",
                    "results": [{
                        "path": "src/lib.rs",
                        "line": 7,
                        "source": "lexical"
                    }]
                }
            }]
        });

        let evidence = collect_evidence(&packet, true);

        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].source_store, "substrate_index");
        assert_eq!(evidence[0].provenance[0].source_store, "substrate_index");
        assert_eq!(evidence[0].span.as_ref().unwrap().start_line, 7);
    }
}
