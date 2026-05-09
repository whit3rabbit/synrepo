use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{config::Config, pipeline::context_metrics, pipeline::writer::acquire_writer_lock};

use super::atomic::{write_planned_files, PlannedFile};
use super::caps::validate_edit_caps;
use super::diagnostics::post_edit_diagnostics;
use super::plan::{reject_overlaps, PlannedEdit};
use super::roots::{resolve_edit_path, PRIMARY_ROOT_ID};
use super::runtime::{suppress_watch_events, writer_lock_conflict_json};
use super::{anchor_manager, PreparedAnchorState};
use crate::surface::mcp::{helpers::render_result, SynrepoState};

use super::prepare::hash_bytes;

const DEFAULT_MAX_LINES: usize = 1_000;
const HARD_MAX_LINES: usize = 5_000;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplyAnchorEditsParams {
    pub repo_root: Option<PathBuf>,
    pub edits: Vec<AnchorEditRequest>,
    /// Optional built-in diagnostics budget. No caller-provided commands run.
    #[serde(default)]
    pub diagnostics_budget: Option<String>,
    #[serde(default)]
    pub max_lines: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct AnchorEditRequest {
    pub task_id: String,
    pub anchor_state_version: String,
    pub path: String,
    #[serde(default)]
    pub root_id: Option<String>,
    pub content_hash: String,
    pub anchor: String,
    #[serde(default)]
    pub end_anchor: Option<String>,
    pub edit_type: String,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EditFileKey {
    root_id: String,
    path: String,
}

pub fn handle_apply_anchor_edits(state: &SynrepoState, params: ApplyAnchorEditsParams) -> String {
    render_result(apply_anchor_edits(state, params))
}

fn apply_anchor_edits(
    state: &SynrepoState,
    params: ApplyAnchorEditsParams,
) -> anyhow::Result<serde_json::Value> {
    if params.edits.is_empty() {
        anyhow::bail!("edits must contain at least one edit");
    }
    let max_lines = params.max_lines.unwrap_or(DEFAULT_MAX_LINES);
    if !(1..=HARD_MAX_LINES).contains(&max_lines) {
        anyhow::bail!("max_lines must be between 1 and {HARD_MAX_LINES}");
    }
    validate_edit_caps(&params.edits)?;
    let submitted_lines = params
        .edits
        .iter()
        .filter(|edit| edit.edit_type != "delete")
        .filter_map(|edit| edit.text.as_deref())
        .map(|text| text.lines().count())
        .sum::<usize>();
    if submitted_lines > max_lines {
        anyhow::bail!(
            "submitted edit text has {submitted_lines} lines, exceeding max_lines {max_lines}"
        );
    }

    let synrepo_dir = Config::synrepo_dir(&state.repo_root);
    let mut groups: BTreeMap<EditFileKey, Vec<AnchorEditRequest>> = BTreeMap::new();
    let mut resolved_paths: BTreeMap<EditFileKey, PathBuf> = BTreeMap::new();
    for edit in params.edits {
        let resolved = resolve_edit_path(state, &edit.path, edit.root_id.as_deref())?;
        let key = EditFileKey {
            root_id: resolved.root_id.clone(),
            path: resolved.relative.clone(),
        };
        resolved_paths
            .entry(key.clone())
            .or_insert(resolved.absolute.clone());
        groups.entry(key).or_default().push(AnchorEditRequest {
            path: resolved.relative,
            root_id: Some(resolved.root_id),
            ..edit
        });
    }
    let requested_edits_total = groups.values().map(Vec::len).sum::<usize>() as u64;
    let paths_to_suppress = resolved_paths.values().cloned().collect::<Vec<_>>();

    let lock = match acquire_writer_lock(&synrepo_dir) {
        Ok(lock) => lock,
        Err(err) => {
            context_metrics::record_anchored_edit_outcomes_best_effort(
                &synrepo_dir,
                0,
                requested_edits_total,
            );
            return Ok(writer_lock_conflict_json(err));
        }
    };

    suppress_watch_events(&synrepo_dir, &paths_to_suppress);

    let mut file_results = Vec::new();
    let mut planned_files = Vec::new();
    let mut preflight_failed = false;
    for (key, edits) in groups {
        match plan_one_file(
            state,
            &key.root_id,
            &key.path,
            &resolved_paths[&key],
            &edits,
        ) {
            Ok(planned) => planned_files.push(planned),
            Err(err) => {
                preflight_failed = true;
                file_results.push(json!({
                    "path": key.path,
                    "root_id": key.root_id,
                    "status": "rejected",
                    "error": err.to_string(),
                }));
            }
        }
    }
    let (file_results, touched, accepted_edits, rejected_edits, completed) = if preflight_failed {
        file_results.extend(planned_files.iter().map(|planned| {
            json!({
                "path": planned.path,
                "root_id": planned.root_id,
                "status": "not_applied",
                "reason": "cross_file_preflight_failed",
            })
        }));
        (file_results, Vec::new(), 0, requested_edits_total, false)
    } else {
        let outcome = write_planned_files(&planned_files);
        let accepted = if outcome.applied {
            planned_files.iter().map(|file| file.edit_count).sum()
        } else {
            0
        };
        let rejected = if outcome.applied {
            0
        } else {
            requested_edits_total
        };
        (
            outcome.file_results,
            outcome.touched,
            accepted,
            rejected,
            outcome.applied,
        )
    };
    drop(lock);
    context_metrics::record_anchored_edit_outcomes_best_effort(
        &synrepo_dir,
        accepted_edits,
        rejected_edits,
    );

    let diagnostics = if !completed || touched.is_empty() {
        json!({
            "validation": "failed",
            "reconcile": { "status": "not_run", "reason": "no files were written" },
            "test_surface_recommendations": [],
        })
    } else {
        post_edit_diagnostics(
            state,
            &synrepo_dir,
            &touched,
            params.diagnostics_budget.as_deref(),
        )
    };

    Ok(json!({
        "status": if completed { "completed" } else { "rejected" },
        "atomicity": {
            "per_file": true,
            "cross_file": true,
            "message": "multi-file requests preflight every file before writing and roll back prior writes on failure"
        },
        "files": file_results,
        "diagnostics": diagnostics,
    }))
}

fn plan_one_file(
    state: &SynrepoState,
    root_id: &str,
    path: &str,
    prepared_abs_path: &Path,
    edits: &[AnchorEditRequest],
) -> anyhow::Result<PlannedFile> {
    let resolved = resolve_edit_path(state, path, Some(root_id))?;
    if resolved.absolute != prepared_abs_path {
        anyhow::bail!("resolved path changed before edit apply for {path}");
    }
    let abs_path = resolved.absolute;
    let content = fs::read_to_string(&abs_path)
        .map_err(|err| anyhow::anyhow!("failed to read {path}: {err}"))?;
    let original = content.as_bytes().to_vec();
    let current_hash = hash_bytes(content.as_bytes());
    let mut lines: Vec<String> = content.lines().map(ToString::to_string).collect();
    let had_final_newline = content.ends_with('\n');

    let mut planned = Vec::new();
    for edit in edits {
        let state = prepared_state(state, root_id, path, edit)?;
        if edit.content_hash != current_hash || state.content_hash != current_hash {
            anyhow::bail!("stale content hash for {path}");
        }
        let (start, start_text) = find_anchor(&state, &edit.anchor)?;
        let end = match edit.end_anchor.as_deref() {
            Some(anchor) => find_anchor(&state, anchor)?,
            None => (start, start_text.clone()),
        };
        let (end, end_text) = end;
        if end < start {
            anyhow::bail!("end_anchor must follow anchor for {path}");
        }
        verify_line(&lines, start, &start_text, &edit.anchor)?;
        verify_line(
            &lines,
            end,
            &end_text,
            edit.end_anchor.as_deref().unwrap_or(&edit.anchor),
        )?;
        planned.push(PlannedEdit::from_request(edit, start, end)?);
    }
    reject_overlaps(&planned)?;

    planned.sort_by_key(|edit| std::cmp::Reverse(edit.descending_apply_key()));
    for edit in planned {
        edit.apply(&mut lines);
    }

    let mut next = lines.join("\n");
    if had_final_newline && !next.ends_with('\n') {
        next.push('\n');
    }
    let new_hash = hash_bytes(next.as_bytes());
    Ok(PlannedFile {
        root_id: root_id.to_string(),
        path: path.to_string(),
        abs_path,
        original,
        next: next.into_bytes(),
        new_hash,
        edit_count: edits.len() as u64,
    })
}

fn prepared_state(
    state: &SynrepoState,
    root_id: &str,
    path: &str,
    edit: &AnchorEditRequest,
) -> anyhow::Result<PreparedAnchorState> {
    let requested_root = edit.root_id.as_deref().unwrap_or(PRIMARY_ROOT_ID);
    if requested_root != root_id {
        anyhow::bail!("root_id changed before edit apply for {path}");
    }
    anchor_manager()
        .get(
            &state.repo_root,
            root_id,
            &edit.task_id,
            path,
            &edit.content_hash,
            &edit.anchor_state_version,
        )
        .ok_or_else(|| anyhow::anyhow!("anchor session is missing, expired, or stale"))
}

fn find_anchor(state: &PreparedAnchorState, anchor: &str) -> anyhow::Result<(usize, String)> {
    state
        .anchors
        .iter()
        .find(|line| line.anchor == anchor)
        .map(|line| (line.line - 1, line.text.clone()))
        .ok_or_else(|| anyhow::anyhow!("unknown anchor: {anchor}"))
}

fn verify_line(
    lines: &[String],
    anchor_index: usize,
    expected: &str,
    anchor: &str,
) -> anyhow::Result<()> {
    let Some(actual) = lines.get(anchor_index) else {
        anyhow::bail!("anchor {anchor} line is outside current file");
    };
    if actual != expected {
        anyhow::bail!("anchor {anchor} boundary text no longer matches current file");
    }
    Ok(())
}
