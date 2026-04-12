//! Compatibility type definitions for the `.synrepo/` runtime.

use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

/// A known runtime store under `.synrepo/`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StoreId {
    /// Canonical observed-facts store.
    Graph,
    /// Supplemental machine-authored store.
    Overlay,
    /// Rebuildable syntext-backed lexical index.
    Index,
    /// Disposable embeddings cache.
    Embeddings,
    /// Disposable LLM response cache.
    LlmResponsesCache,
    /// Ephemeral runtime state.
    State,
}

impl StoreId {
    /// Every currently known store.
    pub const ALL: [StoreId; 6] = [
        StoreId::Graph,
        StoreId::Overlay,
        StoreId::Index,
        StoreId::Embeddings,
        StoreId::LlmResponsesCache,
        StoreId::State,
    ];

    /// Stable user-facing identifier for this store.
    pub fn as_str(self) -> &'static str {
        match self {
            StoreId::Graph => "graph",
            StoreId::Overlay => "overlay",
            StoreId::Index => "index",
            StoreId::Embeddings => "embeddings",
            StoreId::LlmResponsesCache => "cache/llm-responses",
            StoreId::State => "state",
        }
    }

    /// Relative path for this store inside `.synrepo/`.
    pub fn relative_path(self) -> &'static str {
        self.as_str()
    }

    /// Durability class for this store.
    pub fn class(self) -> StoreClass {
        match self {
            StoreId::Graph => StoreClass::Canonical,
            StoreId::Overlay => StoreClass::Supplemental,
            StoreId::Index => StoreClass::Rebuildable,
            StoreId::Embeddings | StoreId::LlmResponsesCache => StoreClass::Disposable,
            StoreId::State => StoreClass::Ephemeral,
        }
    }

    /// Expected format version for this store.
    pub(crate) fn expected_format_version(self) -> u32 {
        super::STORE_FORMAT_VERSION
    }
}

/// Durability class for a runtime store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StoreClass {
    /// Canonical persisted state that must not be silently discarded.
    Canonical,
    /// Supplemental persisted state that may be invalidated without losing truth.
    Supplemental,
    /// Persisted state that may be rebuilt deterministically.
    Rebuildable,
    /// Disposable cache-like state.
    Disposable,
    /// Ephemeral state that can be cleared and recreated.
    Ephemeral,
}

/// Compatibility action selected for a store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompatAction {
    /// Store is compatible and may be used as-is.
    Continue,
    /// Store should be rebuilt.
    Rebuild,
    /// Store should be invalidated and recreated lazily.
    Invalidate,
    /// Store should be cleared and recreated immediately.
    ClearAndRecreate,
    /// Store requires an explicit migration path.
    MigrateRequired,
    /// Store state is incompatible and usage must be blocked.
    Block,
}

impl CompatAction {
    /// Stable user-facing identifier for this action.
    pub fn as_str(self) -> &'static str {
        match self {
            CompatAction::Continue => "continue",
            CompatAction::Rebuild => "rebuild",
            CompatAction::Invalidate => "invalidate",
            CompatAction::ClearAndRecreate => "clear-and-recreate",
            CompatAction::MigrateRequired => "migrate-required",
            CompatAction::Block => "block",
        }
    }
}

/// One store's compatibility decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompatibilityEntry {
    /// Store identifier.
    pub store_id: StoreId,
    /// Durability class.
    pub class: StoreClass,
    /// Selected action.
    pub action: CompatAction,
    /// Human-readable reason for the selected action.
    pub reason: String,
}

/// Summary of compatibility decisions across current runtime stores.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompatibilityReport {
    /// Snapshot path consulted during evaluation.
    pub snapshot_path: PathBuf,
    /// Per-store compatibility decisions.
    pub entries: Vec<CompatibilityEntry>,
    /// Additional guidance that does not map to a direct store action.
    pub warnings: Vec<String>,
}

impl CompatibilityReport {
    /// Return the full compatibility entry for a specific store.
    pub fn entry_for(&self, store_id: StoreId) -> Option<&CompatibilityEntry> {
        self.entries.iter().find(|entry| entry.store_id == store_id)
    }

    /// Return the selected action for a specific store.
    pub fn action_for(&self, store_id: StoreId) -> CompatAction {
        self.entry_for(store_id)
            .map(|entry| entry.action)
            .unwrap_or(CompatAction::Continue)
    }

    /// Return true when a canonical incompatibility blocks safe progress.
    pub fn has_blocking_actions(&self) -> bool {
        self.entries.iter().any(|entry| {
            matches!(
                entry.action,
                CompatAction::MigrateRequired | CompatAction::Block
            )
        })
    }

    /// Render user-facing guidance lines for non-trivial compatibility outcomes.
    pub fn guidance_lines(&self) -> Vec<String> {
        let mut lines = self
            .entries
            .iter()
            .filter(|entry| entry.action != CompatAction::Continue)
            .map(|entry| {
                format!(
                    "{}: {} because {}",
                    entry.store_id.as_str(),
                    entry.action.as_str(),
                    entry.reason
                )
            })
            .collect::<Vec<_>>();
        lines.extend(self.warnings.iter().cloned());
        lines
    }
}

/// Persisted compatibility snapshot under `.synrepo/state/`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeCompatibilitySnapshot {
    /// Snapshot format version.
    pub snapshot_version: u32,
    /// Expected store format versions at the time of the write.
    pub store_format_versions: BTreeMap<String, u32>,
    /// Config-derived compatibility fingerprints.
    pub config_fingerprints: ConfigFingerprints,
}

/// Stored compatibility fingerprints derived from config.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConfigFingerprints {
    /// Inputs that affect discovery and index contents.
    pub index_inputs: String,
    /// Inputs that affect future graph semantics.
    pub graph_inputs: String,
    /// Inputs that affect future history-derived semantics.
    pub history_inputs: String,
    /// Inputs that surface as advisories only; never trigger rebuild or
    /// invalidate. Cross-link confidence thresholds live here: changing them
    /// is a classifier retune, handled by `revalidate_links`.
    #[serde(default = "default_advisory_inputs")]
    pub advisory_inputs: String,
}

fn default_advisory_inputs() -> String {
    // Empty fingerprint so snapshots written before the field existed still
    // round-trip; the first reconcile rewrites them with the real value.
    String::new()
}

impl ConfigFingerprints {
    /// Derive fingerprints from a runtime configuration.
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            index_inputs: fingerprint(&[
                format!("roots={}", config.roots.join("\u{1f}")),
                format!("max_file_size_bytes={}", config.max_file_size_bytes),
                format!("redact_globs={}", config.redact_globs.join("\u{1f}")),
            ]),
            graph_inputs: fingerprint(&[format!(
                "concept_directories={}",
                config.concept_directories.join("\u{1f}")
            )]),
            history_inputs: fingerprint(&[format!("git_commit_depth={}", config.git_commit_depth)]),
            advisory_inputs: fingerprint(&[
                format!(
                    "cross_link_high={:.4}",
                    config.cross_link_confidence_thresholds.high
                ),
                format!(
                    "cross_link_review_queue={:.4}",
                    config.cross_link_confidence_thresholds.review_queue
                ),
                format!("cross_link_cost_limit={}", config.cross_link_cost_limit),
            ]),
        }
    }
}

fn fingerprint(parts: &[String]) -> String {
    let joined = parts.join("\n");
    hex::encode(blake3::hash(joined.as_bytes()).as_bytes())
}
