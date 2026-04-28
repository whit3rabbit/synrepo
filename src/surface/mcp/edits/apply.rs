use std::{collections::BTreeMap, fs, path::PathBuf};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::{
    config::Config,
    pipeline::writer::{acquire_writer_lock, LockError},
};

use super::diagnostics::post_edit_diagnostics;
use super::{anchor_manager, PreparedAnchorState};
use crate::surface::mcp::{helpers::render_result, SynrepoState};

use super::prepare::{hash_bytes, normalize_rel_path};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplyAnchorEditsParams {
    pub repo_root: Option<PathBuf>,
    pub edits: Vec<AnchorEditRequest>,
    /// Optional built-in diagnostics budget. No caller-provided commands run.
    #[serde(default)]
    pub diagnostics_budget: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct AnchorEditRequest {
    pub task_id: String,
    pub anchor_state_version: String,
    pub path: String,
    pub content_hash: String,
    pub anchor: String,
    #[serde(default)]
    pub end_anchor: Option<String>,
    pub edit_type: String,
    #[serde(default)]
    pub text: Option<String>,
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

    let synrepo_dir = Config::synrepo_dir(&state.repo_root);
    let mut groups: BTreeMap<String, Vec<AnchorEditRequest>> = BTreeMap::new();
    for edit in params.edits {
        let path = normalize_rel_path(&state.repo_root, &edit.path)?;
        groups.entry(path).or_default().push(AnchorEditRequest {
            path: edit.path,
            ..edit
        });
    }

    let lock = match acquire_writer_lock(&synrepo_dir) {
        Ok(lock) => lock,
        Err(err) => {
            return Ok(writer_lock_conflict_json(err));
        }
    };

    let mut file_results = Vec::new();
    let mut touched = Vec::new();
    for (path, edits) in groups {
        match apply_one_file(state, &path, &edits) {
            Ok(new_hash) => {
                touched.push(path.clone());
                file_results.push(json!({
                    "path": path,
                    "status": "applied",
                    "new_content_hash": new_hash,
                }));
            }
            Err(err) => file_results.push(json!({
                "path": path,
                "status": "rejected",
                "error": err.to_string(),
            })),
        }
    }
    drop(lock);

    let diagnostics = if touched.is_empty() {
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
        "status": if touched.is_empty() { "rejected" } else { "completed" },
        "atomicity": {
            "per_file": true,
            "cross_file": false,
            "message": "multi-file requests may have mixed per-file outcomes; no cross-file transaction is claimed"
        },
        "files": file_results,
        "diagnostics": diagnostics,
    }))
}

fn apply_one_file(
    state: &SynrepoState,
    path: &str,
    edits: &[AnchorEditRequest],
) -> anyhow::Result<String> {
    let abs_path = state.repo_root.join(path);
    let content = fs::read_to_string(&abs_path)
        .map_err(|err| anyhow::anyhow!("failed to read {path}: {err}"))?;
    let current_hash = hash_bytes(content.as_bytes());
    let mut lines: Vec<String> = content.lines().map(ToString::to_string).collect();
    let had_final_newline = content.ends_with('\n');

    let mut planned = Vec::new();
    for edit in edits {
        let state = prepared_state(state, path, edit)?;
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

    planned.sort_by(|a, b| b.start.cmp(&a.start).then_with(|| b.end.cmp(&a.end)));
    for edit in planned {
        edit.apply(&mut lines);
    }

    let mut next = lines.join("\n");
    if had_final_newline && !next.ends_with('\n') {
        next.push('\n');
    }
    let new_hash = hash_bytes(next.as_bytes());
    write_file_atomically(&abs_path, next.as_bytes())?;
    Ok(new_hash)
}

fn prepared_state(
    state: &SynrepoState,
    path: &str,
    edit: &AnchorEditRequest,
) -> anyhow::Result<PreparedAnchorState> {
    anchor_manager()
        .get(
            &state.repo_root,
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

#[derive(Clone, Debug)]
enum PlannedKind {
    InsertBefore,
    InsertAfter,
    Replace,
    Delete,
}

#[derive(Clone, Debug)]
struct PlannedEdit {
    start: usize,
    end: usize,
    kind: PlannedKind,
    text: Vec<String>,
}

impl PlannedEdit {
    fn from_request(edit: &AnchorEditRequest, start: usize, end: usize) -> anyhow::Result<Self> {
        let kind = match edit.edit_type.as_str() {
            "insert" | "insert_after" => PlannedKind::InsertAfter,
            "insert_before" => PlannedKind::InsertBefore,
            "replace" => PlannedKind::Replace,
            "delete" => PlannedKind::Delete,
            other => anyhow::bail!("unsupported edit_type: {other}"),
        };
        let text = match kind {
            PlannedKind::Delete => Vec::new(),
            _ => edit
                .text
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("text is required for {}", edit.edit_type))?
                .lines()
                .map(ToString::to_string)
                .collect(),
        };
        Ok(Self {
            start,
            end,
            kind,
            text,
        })
    }

    fn interval(&self) -> (usize, usize) {
        match self.kind {
            PlannedKind::InsertBefore => (self.start, self.start),
            PlannedKind::InsertAfter => (self.start + 1, self.start + 1),
            PlannedKind::Replace | PlannedKind::Delete => (self.start, self.end + 1),
        }
    }

    fn apply(self, lines: &mut Vec<String>) {
        match self.kind {
            PlannedKind::InsertBefore => {
                lines.splice(self.start..self.start, self.text);
            }
            PlannedKind::InsertAfter => {
                let idx = self.start + 1;
                lines.splice(idx..idx, self.text);
            }
            PlannedKind::Replace => {
                lines.splice(self.start..=self.end, self.text);
            }
            PlannedKind::Delete => {
                lines.drain(self.start..=self.end);
            }
        }
    }
}

fn reject_overlaps(planned: &[PlannedEdit]) -> anyhow::Result<()> {
    let mut intervals = planned
        .iter()
        .map(PlannedEdit::interval)
        .collect::<Vec<_>>();
    intervals.sort();
    for pair in intervals.windows(2) {
        let (a_start, a_end) = pair[0];
        let (b_start, b_end) = pair[1];
        if a_end > b_start || (a_start == a_end && b_start == b_end && a_start == b_start) {
            anyhow::bail!("overlapping edits in one file are not allowed");
        }
    }
    Ok(())
}

fn write_file_atomically(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension(format!(
        "{}synrepo-edit-tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!("{ext}."))
            .unwrap_or_default()
    ));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn writer_lock_conflict_json(err: LockError) -> serde_json::Value {
    match err {
        LockError::HeldByOther { pid, lock_path } => json!({
            "status": "writer_lock_conflict",
            "writer_lock": {
                "holder_pid": pid,
                "path": lock_path,
            },
            "files": [],
        }),
        other => json!({
            "status": "writer_lock_conflict",
            "writer_lock": {
                "message": other.to_string(),
            },
            "files": [],
        }),
    }
}
