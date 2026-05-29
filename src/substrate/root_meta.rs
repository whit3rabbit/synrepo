//! Metadata helpers for API-facing discovery root labels.

use std::path::Path;

use crate::config::Config;
use crate::substrate::DiscoveryRoot;

/// Stable metadata describing a discovery root in API responses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootMetadata {
    /// Stable root discriminator used in graph rows.
    pub root_id: String,
    /// Stable root-kind API label.
    pub root_kind: String,
    /// Human-readable root label for compact output.
    pub root_label: String,
    /// Full Git ref for branch snapshot roots.
    pub root_ref: Option<String>,
    /// Commit object id for branch snapshot roots.
    pub root_commit: Option<String>,
    /// Whether callers may edit files through this root.
    pub editable: bool,
    /// True when this root is the primary checkout.
    pub is_primary_root: bool,
}

impl RootMetadata {
    /// Metadata for the primary checkout.
    pub fn primary() -> Self {
        Self {
            root_id: "primary".to_string(),
            root_kind: "primary".to_string(),
            root_label: "primary".to_string(),
            root_ref: None,
            root_commit: None,
            editable: true,
            is_primary_root: true,
        }
    }

    /// Metadata for an already-discovered root.
    pub(crate) fn from_discovery_root(root: &DiscoveryRoot) -> Self {
        let root_kind = root.kind.as_str().to_string();
        let root_label = root
            .ref_name
            .as_ref()
            .map(|name| format!("branch:{name}"))
            .unwrap_or_else(|| root_kind.clone());
        Self {
            root_id: root.discriminant.clone(),
            root_kind,
            root_label,
            root_ref: root.ref_name.clone(),
            root_commit: root.commit.clone(),
            editable: root.editable,
            is_primary_root: root.discriminant == "primary",
        }
    }
}

/// Resolve metadata for a root id from current discovery roots.
pub fn root_metadata_for(repo_root: &Path, config: &Config, root_id: &str) -> RootMetadata {
    crate::substrate::discover_roots(repo_root, config)
        .into_iter()
        .find(|root| root.discriminant == root_id)
        .map(|root| RootMetadata::from_discovery_root(&root))
        .unwrap_or_else(|| {
            let mut meta = RootMetadata::primary();
            meta.root_id = root_id.to_string();
            meta.is_primary_root = root_id == "primary";
            meta
        })
}

/// Resolve metadata when compiler context may not carry repo config.
pub fn root_metadata_from_optional(
    repo_root: Option<&Path>,
    config: Option<&Config>,
    root_id: &str,
) -> RootMetadata {
    match (repo_root, config) {
        (Some(repo_root), Some(config)) => root_metadata_for(repo_root, config, root_id),
        _ if root_id == "primary" => RootMetadata::primary(),
        _ => {
            let mut meta = RootMetadata::primary();
            meta.root_id = root_id.to_string();
            meta.is_primary_root = false;
            meta
        }
    }
}
