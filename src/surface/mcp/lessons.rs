use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    config::Config,
    overlay::{
        AgentNoteConfidence, AgentNoteEvidence, AgentNoteSourceHash, AgentNoteTarget,
        AgentNoteTargetKind,
    },
    store::overlay::SqliteOverlayStore,
    surface::lessons::{self, LessonAdd, LessonQuery},
};

use super::helpers::render_result;
use super::limits::{
    bounded_limit_value, check_chars, check_len, MAX_NOTE_CLAIM_CHARS, MAX_NOTE_EVIDENCE,
    MAX_NOTE_SOURCE_HASHES,
};
use super::SynrepoState;

fn default_actor() -> String {
    "mcp-agent".to_string()
}

fn default_confidence() -> AgentNoteConfidence {
    AgentNoteConfidence::Medium
}

fn default_limit() -> usize {
    lessons::DEFAULT_LESSON_LIMIT
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LessonAddParams {
    pub repo_root: Option<std::path::PathBuf>,
    #[serde(default)]
    pub target_kind: Option<AgentNoteTargetKind>,
    #[serde(default)]
    pub target: Option<String>,
    pub claim: String,
    #[serde(default = "default_actor")]
    pub created_by: String,
    #[serde(default = "default_confidence")]
    pub confidence: AgentNoteConfidence,
    #[serde(default)]
    pub evidence: Vec<AgentNoteEvidence>,
    #[serde(default)]
    pub source_hashes: Vec<AgentNoteSourceHash>,
    #[serde(default)]
    pub graph_revision: Option<u64>,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LessonSearchParams {
    pub repo_root: Option<std::path::PathBuf>,
    pub query: String,
    #[serde(default)]
    pub target_kind: Option<AgentNoteTargetKind>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub include_hidden: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LessonListParams {
    pub repo_root: Option<std::path::PathBuf>,
    #[serde(default)]
    pub target_kind: Option<AgentNoteTargetKind>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub include_hidden: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LessonForgetParams {
    pub repo_root: Option<std::path::PathBuf>,
    pub lesson_id: String,
    #[serde(default = "default_actor")]
    pub actor: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LessonVerifyParams {
    pub repo_root: Option<std::path::PathBuf>,
    pub lesson_id: String,
    #[serde(default = "default_actor")]
    pub actor: String,
    #[serde(default)]
    pub graph_revision: Option<u64>,
}

pub fn handle_lesson_add(state: &SynrepoState, params: LessonAddParams) -> String {
    render_result(lesson_add_result(state, params))
}

pub fn handle_lesson_search(state: &SynrepoState, params: LessonSearchParams) -> String {
    render_result(lesson_search_result(state, params))
}

pub fn handle_lesson_list(state: &SynrepoState, params: LessonListParams) -> String {
    render_result(lesson_list_result(state, params))
}

pub fn handle_lesson_forget(state: &SynrepoState, params: LessonForgetParams) -> String {
    render_result(lesson_forget_result(state, params))
}

pub fn handle_lesson_verify(state: &SynrepoState, params: LessonVerifyParams) -> String {
    render_result(lesson_verify_result(state, params))
}

fn lesson_add_result(
    state: &SynrepoState,
    params: LessonAddParams,
) -> anyhow::Result<serde_json::Value> {
    validate_lesson_payload(
        &params.claim,
        params.evidence.len(),
        params.source_hashes.len(),
    )?;
    let target = resolve_add_target(params.target_kind, params.target)?;
    let ttl_seconds = params
        .ttl_seconds
        .map(|seconds| lessons::validate_ttl_seconds(seconds).map(|()| seconds))
        .transpose()?;
    super::notes::with_overlay_for_write(state, "lesson_add", |overlay| {
        let lesson = lessons::add_lesson(
            Some(&state.repo_root),
            overlay,
            LessonAdd {
                target_kind: target.kind,
                target: target.id,
                claim: params.claim,
                created_by: params.created_by,
                confidence: params.confidence,
                evidence: params.evidence,
                source_hashes: params.source_hashes,
                graph_revision: params.graph_revision,
                ttl_seconds,
            },
        )?;
        Ok(serde_json::to_value(lesson)?)
    })
}

fn lesson_search_result(
    state: &SynrepoState,
    params: LessonSearchParams,
) -> anyhow::Result<serde_json::Value> {
    check_chars("query", &params.query, MAX_NOTE_CLAIM_CHARS)?;
    let lessons = read_lessons(
        state,
        Some(params.query),
        params.target_kind,
        params.target,
        params.limit,
        params.include_hidden,
    )?;
    Ok(serde_json::to_value(lessons)?)
}

fn lesson_list_result(
    state: &SynrepoState,
    params: LessonListParams,
) -> anyhow::Result<serde_json::Value> {
    let lessons = read_lessons(
        state,
        None,
        params.target_kind,
        params.target,
        params.limit,
        params.include_hidden,
    )?;
    Ok(serde_json::to_value(lessons)?)
}

fn lesson_forget_result(
    state: &SynrepoState,
    params: LessonForgetParams,
) -> anyhow::Result<serde_json::Value> {
    super::notes::with_overlay_for_write(state, "lesson_forget", |overlay| {
        let lesson = lessons::forget_lesson(
            Some(&state.repo_root),
            overlay,
            &params.lesson_id,
            &params.actor,
            params.reason.as_deref(),
        )?;
        Ok(serde_json::to_value(lesson)?)
    })
}

fn lesson_verify_result(
    state: &SynrepoState,
    params: LessonVerifyParams,
) -> anyhow::Result<serde_json::Value> {
    super::notes::with_overlay_for_write(state, "lesson_verify", |overlay| {
        let lesson = lessons::verify_lesson(
            Some(&state.repo_root),
            overlay,
            &params.lesson_id,
            &params.actor,
            params.graph_revision,
        )?;
        Ok(serde_json::to_value(lesson)?)
    })
}

fn read_lessons(
    state: &SynrepoState,
    query: Option<String>,
    target_kind: Option<AgentNoteTargetKind>,
    target: Option<String>,
    limit: usize,
    include_hidden: bool,
) -> anyhow::Result<Vec<lessons::LessonView>> {
    state.require_overlay_materialized()?;
    let overlay =
        SqliteOverlayStore::open_existing(&Config::synrepo_dir(&state.repo_root).join("overlay"))?;
    let limit = bounded_limit_value(
        limit,
        lessons::DEFAULT_LESSON_LIMIT,
        lessons::MAX_LESSON_LIMIT,
    );
    Ok(lessons::search_lessons(
        Some(&state.repo_root),
        &overlay,
        LessonQuery {
            query,
            target_kind,
            target,
            limit,
            include_hidden,
        },
    )?)
}

fn validate_lesson_payload(
    claim: &str,
    evidence_len: usize,
    source_hashes_len: usize,
) -> anyhow::Result<()> {
    check_chars("claim", claim, MAX_NOTE_CLAIM_CHARS)?;
    check_len("evidence", evidence_len, MAX_NOTE_EVIDENCE)?;
    check_len("source_hashes", source_hashes_len, MAX_NOTE_SOURCE_HASHES)?;
    Ok(())
}

fn resolve_add_target(
    kind: Option<AgentNoteTargetKind>,
    target: Option<String>,
) -> anyhow::Result<AgentNoteTarget> {
    let kind = match kind {
        Some(kind) => kind,
        None if target.is_none() => AgentNoteTargetKind::Repo,
        None => AgentNoteTargetKind::Path,
    };
    let id = match (kind, target) {
        (AgentNoteTargetKind::Repo, None) => ".".to_string(),
        (_, Some(target)) if !target.trim().is_empty() => target.trim().to_string(),
        _ => anyhow::bail!("target is required unless target_kind=repo is used"),
    };
    Ok(AgentNoteTarget { kind, id })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::{bootstrap::bootstrap, config::Config};

    use super::*;

    fn make_state() -> (tempfile::TempDir, SynrepoState) {
        let repo = tempdir().unwrap();
        std::fs::write(repo.path().join("lib.rs"), "pub fn remembered() {}\n").unwrap();
        bootstrap(repo.path(), None, false).unwrap();
        let state = SynrepoState {
            config: Config::load(repo.path()).unwrap(),
            repo_root: repo.path().to_path_buf(),
        };
        (repo, state)
    }

    #[test]
    fn mcp_lessons_round_trip_with_public_shape() {
        let (_repo, state) = make_state();

        let add = handle_lesson_add(
            &state,
            LessonAddParams {
                repo_root: None,
                target_kind: None,
                target: None,
                claim: "Remember that lessons are advisory.".to_string(),
                created_by: "mcp-test".to_string(),
                confidence: AgentNoteConfidence::Medium,
                evidence: vec![AgentNoteEvidence {
                    kind: "text".to_string(),
                    id: "bounded evidence".to_string(),
                }],
                source_hashes: Vec::new(),
                graph_revision: None,
                ttl_seconds: Some(60),
            },
        );
        let value: serde_json::Value = serde_json::from_str(&add).unwrap();
        assert_eq!(value["source_store"], "overlay");
        assert_eq!(value["advisory"], true);
        assert_eq!(value["target_kind"], "repo");
        assert_eq!(value["freshness"], "fresh");
        assert!(value["expires_at"].is_string());
        assert_eq!(value["evidence"].as_array().unwrap().len(), 1);

        let listed = handle_lesson_list(
            &state,
            LessonListParams {
                repo_root: None,
                target_kind: None,
                target: None,
                include_hidden: false,
                limit: 10,
            },
        );
        let lessons: serde_json::Value = serde_json::from_str(&listed).unwrap();
        assert_eq!(lessons.as_array().unwrap().len(), 1);
        assert_eq!(lessons[0]["lesson_id"], value["lesson_id"]);
    }

    #[test]
    fn mcp_lesson_add_rejects_bad_ttl_as_invalid_parameter() {
        let (_repo, state) = make_state();

        let out = handle_lesson_add(
            &state,
            LessonAddParams {
                repo_root: None,
                target_kind: None,
                target: None,
                claim: "Bad TTL".to_string(),
                created_by: "mcp-test".to_string(),
                confidence: AgentNoteConfidence::Medium,
                evidence: Vec::new(),
                source_hashes: Vec::new(),
                graph_revision: None,
                ttl_seconds: Some(0),
            },
        );
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "INVALID_PARAMETER");
    }
}
