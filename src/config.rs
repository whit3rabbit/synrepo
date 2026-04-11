//! Configuration loading. Reads `.synrepo/config.toml`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Which operational mode synrepo runs in.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Bootstrap defaults here when repository inspection does not find
    /// rationale markdown under the configured concept directories.
    /// Synthesis runs automatically in the background and writes to the
    /// overlay. Concept nodes are disabled unless human-authored concept
    /// directories exist.
    Auto,
    /// Bootstrap recommends or selects this when repository inspection
    /// finds rationale markdown under the configured concept directories,
    /// unless an explicit or already-configured mode is kept instead.
    /// Synthesis proposals go to a review queue. Concept nodes are
    /// enabled when human-authored ADR directories exist.
    Curated,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Auto
    }
}

/// Top-level config read from `.synrepo/config.toml`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Operational mode. Bootstrap prefers an explicit `--mode`, otherwise
    /// it keeps an existing configured mode or falls back to repository
    /// inspection before defaulting to `auto`.
    #[serde(default)]
    pub mode: Mode,

    /// Roots to index, relative to the repo root. Default is `["."]`.
    #[serde(default = "default_roots")]
    pub roots: Vec<String>,

    /// Directories that contain human-authored concept/decision files.
    /// If empty, concept nodes are disabled in auto mode.
    #[serde(default = "default_concept_dirs")]
    pub concept_directories: Vec<String>,

    /// Git history depth for mining co-change, ownership, blame.
    #[serde(default = "default_git_commit_depth")]
    pub git_commit_depth: u32,

    /// Maximum file size in bytes for indexing. Files above this are skipped.
    #[serde(default = "default_max_file_size")]
    pub max_file_size_bytes: u64,

    /// Paths matching these globs are skipped entirely (e.g. secrets).
    #[serde(default = "default_redact_globs")]
    pub redact_globs: Vec<String>,
}

fn default_roots() -> Vec<String> {
    vec![".".to_string()]
}

fn default_concept_dirs() -> Vec<String> {
    vec![
        "docs/concepts".to_string(),
        "docs/adr".to_string(),
        "docs/decisions".to_string(),
    ]
}

fn default_git_commit_depth() -> u32 {
    500
}

fn default_max_file_size() -> u64 {
    1 * 1024 * 1024 // 1 MB
}

fn default_redact_globs() -> Vec<String> {
    vec![
        "**/secrets/**".to_string(),
        "**/*.env*".to_string(),
        "**/*-private.md".to_string(),
    ]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            roots: default_roots(),
            concept_directories: default_concept_dirs(),
            git_commit_depth: default_git_commit_depth(),
            max_file_size_bytes: default_max_file_size(),
            redact_globs: default_redact_globs(),
        }
    }
}

impl Config {
    /// Load config from `repo_root/.synrepo/config.toml`. If the file
    /// doesn't exist, return defaults.
    pub fn load(repo_root: &Path) -> crate::Result<Self> {
        let path = repo_root.join(".synrepo/config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).map_err(|e| crate::Error::Config(e.to_string()))
    }

    /// Path to the `.synrepo/` directory for a given repo root.
    pub fn synrepo_dir(repo_root: &Path) -> PathBuf {
        repo_root.join(".synrepo")
    }
}
