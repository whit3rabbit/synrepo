use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    config::Config,
    overlay::{
        AgentNote, AgentNoteConfidence, AgentNoteEvidence, AgentNoteQuery, AgentNoteSourceHash,
        AgentNoteTarget, AgentNoteTargetKind, OverlayStore,
    },
    pipeline::writer::{acquire_write_admission, map_lock_error},
    store::overlay::SqliteOverlayStore,
};

use super::helpers::render_result;
use super::limits::{
    check_chars, check_len, MAX_NOTE_CLAIM_CHARS, MAX_NOTE_EVIDENCE, MAX_NOTE_SOURCE_HASHES,
};
use super::SynrepoState;

fn default_actor() -> String {
    "mcp-agent".to_string()
}

fn default_confidence() -> AgentNoteConfidence {
    AgentNoteConfidence::Medium
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteAddParams {
    pub repo_root: Option<std::path::PathBuf>,
    pub target_kind: AgentNoteTargetKind,
    pub target: String,
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
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteLinkParams {
    pub repo_root: Option<std::path::PathBuf>,
    pub from_note: String,
    pub to_note: String,
    #[serde(default = "default_actor")]
    pub actor: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteSupersedeParams {
    pub repo_root: Option<std::path::PathBuf>,
    pub old_note: String,
    pub target_kind: AgentNoteTargetKind,
    pub target: String,
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
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteForgetParams {
    pub repo_root: Option<std::path::PathBuf>,
    pub note_id: String,
    #[serde(default = "default_actor")]
    pub actor: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteVerifyParams {
    pub repo_root: Option<std::path::PathBuf>,
    pub note_id: String,
    #[serde(default = "default_actor")]
    pub actor: String,
    #[serde(default)]
    pub graph_revision: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NotesParams {
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

pub fn handle_note_add(state: &SynrepoState, params: NoteAddParams) -> String {
    render_result(note_add_result(state, params))
}

pub fn handle_note_link(state: &SynrepoState, params: NoteLinkParams) -> String {
    render_result(note_link_result(state, params))
}

pub fn handle_note_supersede(state: &SynrepoState, params: NoteSupersedeParams) -> String {
    render_result(note_supersede_result(state, params))
}

pub fn handle_note_forget(state: &SynrepoState, params: NoteForgetParams) -> String {
    render_result(note_forget_result(state, params))
}

pub fn handle_note_verify(state: &SynrepoState, params: NoteVerifyParams) -> String {
    render_result(note_verify_result(state, params))
}

pub fn handle_notes(state: &SynrepoState, params: NotesParams) -> String {
    render_result(notes_result(state, params))
}

fn note_add_result(
    state: &SynrepoState,
    params: NoteAddParams,
) -> anyhow::Result<serde_json::Value> {
    validate_note_payload(
        &params.claim,
        params.evidence.len(),
        params.source_hashes.len(),
    )?;
    let mut note = AgentNote::new(
        AgentNoteTarget {
            kind: params.target_kind,
            id: params.target,
        },
        params.claim,
        params.created_by,
        params.confidence,
    );
    note.evidence = params.evidence;
    note.source_hashes = params.source_hashes;
    note.graph_revision = params.graph_revision;
    with_overlay_for_write(state, "note_add", |overlay| {
        Ok(serde_json::to_value(overlay.insert_note(note)?)?)
    })
}

fn note_link_result(
    state: &SynrepoState,
    params: NoteLinkParams,
) -> anyhow::Result<serde_json::Value> {
    with_overlay_for_write(state, "note_link", |overlay| {
        overlay.link_note(&params.from_note, &params.to_note, &params.actor)?;
        Ok(serde_json::json!({
            "linked": true,
            "from_note": params.from_note,
            "to_note": params.to_note,
            "source_store": "overlay",
            "advisory": true
        }))
    })
}

fn note_supersede_result(
    state: &SynrepoState,
    params: NoteSupersedeParams,
) -> anyhow::Result<serde_json::Value> {
    validate_note_payload(
        &params.claim,
        params.evidence.len(),
        params.source_hashes.len(),
    )?;
    let mut replacement = AgentNote::new(
        AgentNoteTarget {
            kind: params.target_kind,
            id: params.target,
        },
        params.claim,
        params.created_by.clone(),
        params.confidence,
    );
    replacement.evidence = params.evidence;
    replacement.source_hashes = params.source_hashes;
    replacement.graph_revision = params.graph_revision;
    with_overlay_for_write(state, "note_supersede", |overlay| {
        Ok(serde_json::to_value(overlay.supersede_note(
            &params.old_note,
            replacement,
            &params.created_by,
        )?)?)
    })
}

fn validate_note_payload(
    claim: &str,
    evidence_len: usize,
    source_hashes_len: usize,
) -> anyhow::Result<()> {
    check_chars("claim", claim, MAX_NOTE_CLAIM_CHARS)?;
    check_len("evidence", evidence_len, MAX_NOTE_EVIDENCE)?;
    check_len("source_hashes", source_hashes_len, MAX_NOTE_SOURCE_HASHES)?;
    Ok(())
}

fn note_forget_result(
    state: &SynrepoState,
    params: NoteForgetParams,
) -> anyhow::Result<serde_json::Value> {
    with_overlay_for_write(state, "note_forget", |overlay| {
        overlay.forget_note(&params.note_id, &params.actor, params.reason.as_deref())?;
        Ok(serde_json::json!({
            "forgotten": true,
            "note_id": params.note_id,
            "source_store": "overlay",
            "advisory": true
        }))
    })
}

fn note_verify_result(
    state: &SynrepoState,
    params: NoteVerifyParams,
) -> anyhow::Result<serde_json::Value> {
    with_overlay_for_write(state, "note_verify", |overlay| {
        Ok(serde_json::to_value(overlay.verify_note(
            &params.note_id,
            &params.actor,
            params.graph_revision,
        )?)?)
    })
}

fn notes_result(state: &SynrepoState, params: NotesParams) -> anyhow::Result<serde_json::Value> {
    let synrepo_dir = Config::synrepo_dir(&state.repo_root);
    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    let notes = overlay.query_notes(AgentNoteQuery {
        target_kind: params.target_kind,
        target_id: params.target,
        include_forgotten: params.include_hidden,
        include_superseded: params.include_hidden,
        include_invalid: params.include_hidden,
        limit: params.limit,
    })?;
    Ok(serde_json::to_value(notes)?)
}

pub(crate) fn attach_agent_notes(
    state: &SynrepoState,
    json: &mut serde_json::Value,
    node_id: crate::core::ids::NodeId,
) -> anyhow::Result<()> {
    let target_kind = match node_id {
        crate::core::ids::NodeId::File(_) => AgentNoteTargetKind::File,
        crate::core::ids::NodeId::Symbol(_) => AgentNoteTargetKind::Symbol,
        crate::core::ids::NodeId::Concept(_) => AgentNoteTargetKind::Concept,
    };
    let synrepo_dir = Config::synrepo_dir(&state.repo_root);
    let overlay = match SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")) {
        Ok(overlay) => overlay,
        Err(_) => return Ok(()),
    };
    let notes = overlay.query_notes(AgentNoteQuery {
        target_kind: Some(target_kind),
        target_id: Some(node_id.to_string()),
        include_forgotten: false,
        include_superseded: false,
        include_invalid: false,
        limit: 5,
    })?;
    if let serde_json::Value::Object(map) = json {
        map.insert("advisory_notes".to_string(), serde_json::to_value(notes)?);
    }
    Ok(())
}

fn with_overlay_for_write<R>(
    state: &SynrepoState,
    operation: &'static str,
    f: impl FnOnce(&mut SqliteOverlayStore) -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    let synrepo_dir = Config::synrepo_dir(&state.repo_root);
    let _lock = acquire_write_admission(&synrepo_dir, operation)
        .map_err(|err| map_lock_error(operation, err))?;
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay"))?;
    f(&mut overlay)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::config::Config;

    use super::*;

    #[test]
    fn mcp_notes_round_trip_with_advisory_labels() {
        let repo = tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("src/lib.rs"), "pub fn noted() {}\n").unwrap();
        crate::bootstrap::bootstrap(repo.path(), None, false).unwrap();
        let state = SynrepoState {
            config: Config::load(repo.path()).unwrap(),
            repo_root: repo.path().to_path_buf(),
        };

        let add = handle_note_add(
            &state,
            NoteAddParams {
                repo_root: None,
                target_kind: AgentNoteTargetKind::Path,
                target: "src/lib.rs".to_string(),
                claim: "Advisory only.".to_string(),
                created_by: "mcp-test".to_string(),
                confidence: AgentNoteConfidence::Medium,
                evidence: Vec::new(),
                source_hashes: Vec::new(),
                graph_revision: None,
            },
        );
        let value: serde_json::Value = serde_json::from_str(&add).unwrap();
        assert_eq!(value["source_store"], "overlay");
        assert_eq!(value["advisory"], true);

        let listed = handle_notes(
            &state,
            NotesParams {
                repo_root: None,
                target_kind: Some(AgentNoteTargetKind::Path),
                target: Some("src/lib.rs".to_string()),
                include_hidden: false,
                limit: 10,
            },
        );
        let notes: serde_json::Value = serde_json::from_str(&listed).unwrap();
        assert_eq!(notes.as_array().unwrap().len(), 1);
        assert_eq!(notes[0]["source_store"], "overlay");
        assert_eq!(notes[0]["advisory"], true);

        let card = crate::surface::mcp::cards::handle_card(
            &state,
            "src/lib.rs".to_string(),
            "tiny".to_string(),
            None,
            false,
        );
        let card: serde_json::Value = serde_json::from_str(&card).unwrap();
        assert!(card.get("advisory_notes").is_none());
        assert_eq!(card["source_store"], "graph");
    }
}
