//! Folder-picker sub-view for the Explain tab. Enumerates top-level repo
//! directories, seeds default selection from a heuristic (≥1 parser-supported
//! file) or a persisted prior selection at
//! `.synrepo/state/explain-scope.json`, and exposes the state + key
//! handling primitives the render loop needs.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyModifiers};
use serde::{Deserialize, Serialize};

use super::{AppState, ExplainMode};
use crate::config::Config;
use crate::substrate::{classify::FileClass, discover::discover};

/// Persisted scope file relative to the repo root.
const SCOPE_FILE: &str = ".synrepo/state/explain-scope.json";

/// One top-level directory entry in the picker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderEntry {
    /// Repo-relative path, trailing-slash terminated (e.g. `"src/"`).
    pub path: String,
    /// Count of indexable files bucketed to this segment.
    pub indexable_count: usize,
    /// Of those, how many are `FileClass::SupportedCode` (parser-supported).
    pub supported_count: usize,
    /// Current toggle state.
    pub checked: bool,
}

/// Folder-picker widget state. Owned by `AppState` while the picker is open.
#[derive(Clone, Debug)]
pub struct FolderPickerState {
    /// Top-level entries, sorted alphabetically.
    pub folders: Vec<FolderEntry>,
    /// Cursor row. Clamped to `folders.len().saturating_sub(1)` on updates.
    pub cursor: usize,
}

impl FolderPickerState {
    /// Build picker state by scanning the repo and merging with any persisted
    /// prior selection. When a prior selection exists it overrides the
    /// "looks like source" heuristic.
    pub fn build(repo_root: &Path, config: &Config) -> crate::Result<Self> {
        let entries = enumerate_folders(repo_root, config)?;
        let prior = load_scope(repo_root);
        let folders = apply_selection(entries, prior.as_deref());
        Ok(Self { folders, cursor: 0 })
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.folders.len()
    }

    /// True when there is nothing to render.
    pub fn is_empty(&self) -> bool {
        self.folders.is_empty()
    }

    /// Move the cursor up `rows`, saturating at 0.
    pub fn cursor_up(&mut self, rows: usize) {
        self.cursor = self.cursor.saturating_sub(rows);
    }

    /// Move the cursor down `rows`, clamped to the last row.
    pub fn cursor_down(&mut self, rows: usize) {
        if self.folders.is_empty() {
            self.cursor = 0;
            return;
        }
        let last = self.folders.len() - 1;
        self.cursor = self.cursor.saturating_add(rows).min(last);
    }

    /// Flip the checkbox at the cursor.
    pub fn toggle_cursor(&mut self) {
        if let Some(entry) = self.folders.get_mut(self.cursor) {
            entry.checked = !entry.checked;
        }
    }

    /// Repo-relative paths of every checked folder, trailing slash preserved.
    pub fn selected_paths(&self) -> Vec<String> {
        self.folders
            .iter()
            .filter(|e| e.checked)
            .map(|e| e.path.clone())
            .collect()
    }
}

/// Enumerate top-level directories that contain at least one indexable file,
/// sorted alphabetically.
fn enumerate_folders(repo_root: &Path, config: &Config) -> crate::Result<Vec<FolderEntry>> {
    let files = discover(repo_root, config)?;
    let mut buckets: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for file in files {
        let Some(segment) = first_path_segment(&file.relative_path) else {
            continue;
        };
        let entry = buckets.entry(segment).or_insert((0, 0));
        entry.0 += 1;
        if matches!(file.class, FileClass::SupportedCode { .. }) {
            entry.1 += 1;
        }
    }
    Ok(buckets
        .into_iter()
        .map(|(segment, (indexable, supported))| FolderEntry {
            path: format!("{segment}/"),
            indexable_count: indexable,
            supported_count: supported,
            // Default-check: parser-supported files present. Overridden by
            // persisted prior selection in `apply_selection`.
            checked: supported > 0,
        })
        .collect())
}

/// Return the first path segment of a repo-relative path, or `None` when the
/// path has no `/` separator (top-level file).
fn first_path_segment(relative_path: &str) -> Option<String> {
    let normalized = relative_path.replace('\\', "/");
    let (head, rest) = normalized.split_once('/')?;
    if head.is_empty() || rest.is_empty() {
        return None;
    }
    Some(head.to_string())
}

/// Overlay a persisted selection on top of the heuristic defaults. Entries
/// absent from the file keep the heuristic value; entries present in the file
/// are force-checked.
fn apply_selection(mut entries: Vec<FolderEntry>, prior: Option<&[String]>) -> Vec<FolderEntry> {
    let Some(prior) = prior else {
        return entries;
    };
    for entry in entries.iter_mut() {
        entry.checked = prior.iter().any(|p| normalize_prefix(p) == entry.path);
    }
    entries
}

/// Normalize a persisted path to the `"segment/"` form used by `FolderEntry`.
fn normalize_prefix(path: &str) -> String {
    let mut s = path.replace('\\', "/");
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

impl AppState {
    /// Open the folder picker. Uses cached config-derived data; on a scan
    /// failure we toast the error and leave the tab as-is rather than crash.
    pub(super) fn open_folder_picker(&mut self) {
        let invalid_scope = load_scope_state(&self.repo_root).invalid;
        if invalid_scope {
            self.set_toast("saved folder scope was invalid; using defaults");
        }
        match Config::load(&self.repo_root) {
            Ok(config) => match FolderPickerState::build(&self.repo_root, &config) {
                Ok(state) if state.is_empty() => {
                    self.set_toast("No indexable top-level folders to choose from.");
                }
                Ok(state) => self.picker = Some(state),
                Err(err) => self.set_toast(format!("folder picker: {err}")),
            },
            Err(err) => self.set_toast(format!("folder picker: config load failed ({err})")),
        }
    }

    /// Modal key handling for the folder picker. Returns `Some(true)` when the
    /// key was consumed, or `None` when the outer dispatch should try to
    /// handle it. Quit and tab-switch keys fall through so the operator can
    /// always escape the modal.
    pub(super) fn handle_picker_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> Option<bool> {
        if matches!(
            code,
            KeyCode::Char('q')
                | KeyCode::Tab
                | KeyCode::BackTab
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Char('1')
                | KeyCode::Char('2')
                | KeyCode::Char('3')
                | KeyCode::Char('4')
                | KeyCode::Char('5')
                | KeyCode::Char('6')
                | KeyCode::Char('7')
                | KeyCode::Char('8')
        ) {
            return None;
        }
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            return None;
        }
        let picker = self
            .picker
            .as_mut()
            .expect("handle_picker_key requires picker to be Some");
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                picker.cursor_up(1);
                Some(true)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                picker.cursor_down(1);
                Some(true)
            }
            KeyCode::Char(' ') => {
                picker.toggle_cursor();
                Some(true)
            }
            KeyCode::Enter => {
                let paths = picker.selected_paths();
                if paths.is_empty() {
                    self.set_toast("Select at least one folder, or press Esc to cancel.");
                    return Some(true);
                }
                if let Err(err) = save_scope(&self.repo_root, &paths) {
                    self.set_toast(format!("folder picker: save failed ({err})"));
                    return Some(true);
                }
                self.picker = None;
                self.queue_explain(ExplainMode::Paths(paths));
                Some(true)
            }
            KeyCode::Esc => {
                self.picker = None;
                Some(true)
            }
            _ => Some(true),
        }
    }
}

/// On-disk schema for `.synrepo/state/explain-scope.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ScopeState {
    #[serde(default)]
    paths: Vec<String>,
}

/// Absolute path to the scope file.
fn scope_path(repo_root: &Path) -> PathBuf {
    repo_root.join(SCOPE_FILE)
}

/// Read the persisted scope. Returns `None` on any IO or parse failure so the
/// picker falls back to the heuristic rather than crashing the tab.
pub fn load_scope(repo_root: &Path) -> Option<Vec<String>> {
    load_scope_state(repo_root).paths
}

struct ScopeLoad {
    paths: Option<Vec<String>>,
    invalid: bool,
}

fn load_scope_state(repo_root: &Path) -> ScopeLoad {
    let path = scope_path(repo_root);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return ScopeLoad {
                paths: None,
                invalid: false,
            };
        }
        Err(_) => {
            return ScopeLoad {
                paths: None,
                invalid: true,
            };
        }
    };
    match serde_json::from_str::<ScopeState>(&text) {
        Ok(state) => ScopeLoad {
            paths: Some(state.paths),
            invalid: false,
        },
        Err(_) => ScopeLoad {
            paths: None,
            invalid: true,
        },
    }
}

/// Persist the selection. Creates the parent directory if it does not exist.
pub fn save_scope(repo_root: &Path, paths: &[String]) -> crate::Result<()> {
    let path = scope_path(repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let state = ScopeState {
        paths: paths.to_vec(),
    };
    let text = serde_json::to_string_pretty(&state)
        .map_err(|e| crate::Error::Config(format!("explain-scope serialize: {e}")))?;
    fs::write(&path, text)?;
    Ok(())
}

#[cfg(test)]
#[path = "explain_picker_tests.rs"]
mod tests;
