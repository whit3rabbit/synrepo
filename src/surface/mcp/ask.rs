use serde_json::{json, Value};

use crate::surface::context::compiler::{compile_context_request, grounding_status};
use crate::surface::context::{Confidence, ContextAskRequest, ContextTarget};

use super::compact::OutputMode;
use super::context_pack::{self, ContextPackParams, ContextPackTarget};
use super::helpers::render_result;
use super::SynrepoState;

const SCHEMA_VERSION: u32 = 1;
const MAX_EVIDENCE_ITEMS: usize = 20;

/// Parameters for the `synrepo_ask` tool.
pub type AskParams = ContextAskRequest;

pub fn handle_ask(state: &SynrepoState, params: AskParams) -> String {
    render_result(build_ask_packet(state, params))
}

pub fn build_ask_packet(state: &SynrepoState, params: AskParams) -> anyhow::Result<Value> {
    let plan = compile_context_request(&params)?;
    let pack = context_pack::build_context_pack(
        state,
        ContextPackParams {
            repo_root: params.repo_root.clone(),
            goal: Some(params.ask.clone()),
            targets: to_context_pack_targets(&plan.targets),
            budget: plan.budget_tier.clone(),
            budget_tokens: Some(plan.budget_tokens),
            output_mode: OutputMode::Compact,
            include_tests: plan.include_tests,
            include_notes: plan.include_notes,
            limit: plan.limit,
        },
    )?;
    let cards_used = collect_cards_used(&pack);
    let evidence = collect_evidence(&pack, params.ground.include_spans);
    let status = grounding_status(&params.ground, evidence.len());
    let mut omitted_context_notes = plan.omitted_context_notes.clone();
    append_pack_omissions(&pack, &mut omitted_context_notes);

    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "ask": params.ask,
        "recipe": plan.recipe,
        "answer": compact_answer(&plan.recipe, cards_used.len(), evidence.len(), status),
        "cards_used": cards_used,
        "evidence": evidence,
        "grounding": {
            "mode": params.ground.mode,
            "include_spans": params.ground.include_spans,
            "allow_overlay": params.ground.allow_overlay,
            "status": status,
        },
        "budget": {
            "tier": plan.budget_tier,
            "max_tokens": plan.budget_tokens,
            "target_limit": plan.limit,
        },
        "omitted_context_notes": omitted_context_notes,
        "next_best_tools": plan.next_best_tools,
        "context_packet": pack,
    }))
}

fn to_context_pack_targets(targets: &[ContextTarget]) -> Vec<ContextPackTarget> {
    targets
        .iter()
        .map(|target| ContextPackTarget {
            kind: target.kind.clone(),
            target: target.target.clone(),
            budget: target.budget.clone(),
        })
        .collect()
}

fn compact_answer(
    recipe: &crate::surface::context::ContextRecipe,
    artifact_count: usize,
    evidence_count: usize,
    grounding_status: &str,
) -> String {
    format!(
        "Compiled a {:?} task context with {artifact_count} artifact(s), {evidence_count} evidence item(s), grounding={grounding_status}.",
        recipe
    )
}

fn collect_cards_used(packet: &Value) -> Vec<String> {
    packet
        .get("artifacts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|artifact| artifact.get("status").and_then(Value::as_str) == Some("ok"))
        .filter_map(|artifact| {
            let kind = artifact.get("artifact_type").and_then(Value::as_str)?;
            let target = artifact.get("target").and_then(Value::as_str)?;
            Some(format!("{kind}:{target}"))
        })
        .collect()
}

fn collect_evidence(packet: &Value, include_spans: bool) -> Vec<Value> {
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

fn collect_artifact_evidence(artifact: &Value, include_spans: bool, evidence: &mut Vec<Value>) {
    let content = artifact.get("content").unwrap_or(&Value::Null);
    let (source, line) = source_for_artifact(artifact, content);
    evidence.push(json!({
        "claim": format!(
            "Included {} for {}",
            artifact.get("artifact_type").and_then(Value::as_str).unwrap_or("artifact"),
            artifact.get("target").and_then(Value::as_str).unwrap_or("unknown target")
        ),
        "source": source,
        "span": span_value(include_spans, line),
        "source_store": content
            .get("source_store")
            .and_then(Value::as_str)
            .unwrap_or("graph"),
        "confidence": Confidence::Observed,
    }));
}

fn collect_search_evidence(artifact: &Value, include_spans: bool, evidence: &mut Vec<Value>) {
    let content = artifact.get("content").unwrap_or(&Value::Null);
    let query = content
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("search query");
    if let Some(rows) = content.get("results").and_then(Value::as_array) {
        for row in rows.iter().take(3) {
            push_search_row_evidence(query, row, include_spans, evidence);
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
        evidence.push(json!({
            "claim": format!("Search for `{query}` matched {source}"),
            "source": source,
            "span": span_value(include_spans, line),
            "source_store": content
                .get("source_store")
                .and_then(Value::as_str)
                .unwrap_or("substrate_index"),
            "confidence": Confidence::Observed,
        }));
    }
}

fn push_search_row_evidence(
    query: &str,
    row: &Value,
    include_spans: bool,
    evidence: &mut Vec<Value>,
) {
    let Some(source) = row.get("path").and_then(Value::as_str) else {
        return;
    };
    evidence.push(json!({
        "claim": format!("Search for `{query}` matched {source}"),
        "source": source,
        "span": span_value(include_spans, row.get("line").and_then(Value::as_u64)),
        "source_store": row
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("substrate_index"),
        "confidence": Confidence::Observed,
    }));
}

fn source_for_artifact(artifact: &Value, content: &Value) -> (String, Option<u64>) {
    if let Some(path) = content.get("path").and_then(Value::as_str) {
        return (path.to_string(), None);
    }
    if let Some(defined_at) = content.get("defined_at").and_then(Value::as_str) {
        if let Some((path, line)) = defined_at.rsplit_once(':') {
            if let Ok(line) = line.parse::<u64>() {
                return (path.to_string(), Some(line));
            }
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

fn span_value(include_spans: bool, line: Option<u64>) -> Value {
    if include_spans {
        if let Some(line) = line {
            return json!({ "start_line": line, "end_line": line });
        }
    }
    Value::Null
}

fn append_pack_omissions(packet: &Value, notes: &mut Vec<String>) {
    let Some(omitted) = packet.get("omitted").and_then(Value::as_array) else {
        return;
    };
    for item in omitted.iter().take(5) {
        let target = item
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("unknown target");
        let reason = item
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("omitted");
        notes.push(format!("{target} omitted: {reason}"));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::bootstrap::bootstrap;
    use crate::config::Config;
    use crate::surface::context::{ContextBudget, ContextScope, ContextShape, GroundingOptions};

    use super::*;

    fn make_state() -> (tempfile::TempDir, SynrepoState) {
        let home = tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let dir = tempdir().unwrap();
        let repo = dir.path();
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(
            repo.join("src/lib.rs"),
            "pub fn alpha() {}\npub fn beta() { alpha(); }\n",
        )
        .unwrap();
        bootstrap(repo, None, false).unwrap();
        let state = SynrepoState {
            config: Config::load(repo).unwrap(),
            repo_root: repo.to_path_buf(),
        };
        (dir, state)
    }

    fn request(ask: &str) -> ContextAskRequest {
        ContextAskRequest {
            repo_root: None,
            ask: ask.to_string(),
            scope: ContextScope::default(),
            shape: ContextShape::default(),
            ground: GroundingOptions::default(),
            budget: ContextBudget::default(),
        }
    }

    #[test]
    fn ask_builds_context_packet_with_evidence() {
        let (_dir, state) = make_state();
        let mut params = request("review module");
        params.scope.paths = vec!["src/lib.rs".into()];

        let value = build_ask_packet(&state, params).unwrap();

        assert_eq!(value["schema_version"], 1);
        assert!(value["answer"].as_str().unwrap().contains("task context"));
        assert_eq!(value["grounding"]["status"], "grounded");
        assert_eq!(
            value["context_packet"]["artifacts"][0]["target"],
            "src/lib.rs"
        );
        assert!(value["cards_used"]
            .as_array()
            .is_some_and(|cards| !cards.is_empty()));
        assert_eq!(value["evidence"][0]["source"], "src/lib.rs");
    }

    #[test]
    fn ask_uses_search_when_scope_is_empty() {
        let (_dir, state) = make_state();
        let value = build_ask_packet(&state, request("where is alpha")).unwrap();

        assert!(value["context_packet"]["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact["artifact_type"] == "search"));
    }

    #[test]
    fn ask_review_module_directory_does_not_emit_minimum_context() {
        let (_dir, state) = make_state();
        let mut params = request("review this module");
        params.scope.paths = vec!["src".into()];

        let value = build_ask_packet(&state, params).unwrap();
        let artifacts = value["context_packet"]["artifacts"].as_array().unwrap();

        assert!(artifacts
            .iter()
            .any(|artifact| artifact["artifact_type"] == "module_card"));
        assert!(artifacts
            .iter()
            .any(|artifact| artifact["artifact_type"] == "public_api"));
        assert!(!artifacts
            .iter()
            .any(|artifact| artifact["target_kind"] == "minimum_context"));
    }

    #[test]
    fn ask_release_readiness_can_include_findings_and_activity() {
        let (_dir, state) = make_state();
        let mut params = request("release readiness");
        params.ground.allow_overlay = true;

        let value = build_ask_packet(&state, params).unwrap();
        let artifacts = value["context_packet"]["artifacts"].as_array().unwrap();

        assert!(artifacts
            .iter()
            .any(|artifact| artifact["target_kind"] == "findings"));
        assert!(artifacts
            .iter()
            .any(|artifact| artifact["artifact_type"] == "recent_activity"));
        assert_ne!(value["grounding"]["status"], "insufficient");
    }

    #[test]
    fn ask_security_review_adds_bounded_risky_flow_searches() {
        let (_dir, state) = make_state();
        let value = build_ask_packet(&state, request("security review")).unwrap();
        let artifacts = value["context_packet"]["artifacts"].as_array().unwrap();

        assert!(artifacts
            .iter()
            .any(|artifact| artifact["artifact_type"] == "entrypoints"));
        assert!(artifacts.iter().any(|artifact| {
            artifact["artifact_type"] == "search"
                && artifact["target"].as_str() == Some("Command::new")
        }));
        assert!(artifacts.iter().any(|artifact| {
            artifact["artifact_type"] == "search"
                && artifact["target"].as_str() == Some("TcpStream")
        }));
    }
}
