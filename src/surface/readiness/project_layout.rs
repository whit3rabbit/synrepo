use crate::{config::Config, core::project_layout::detect_project_layout};

use super::{Capability, ReadinessRow, ReadinessState};

pub(super) fn project_layout_row(repo_root: &std::path::Path, config: &Config) -> ReadinessRow {
    let layout = detect_project_layout(repo_root, &config.roots);
    if layout.is_empty() {
        return ReadinessRow {
            capability: Capability::ProjectLayout,
            state: ReadinessState::Disabled,
            detail: "no recognized project manifests".to_string(),
            next_action: None,
        };
    }

    let profiles = layout.profile_labels();
    let detail = if layout.excluded_roots.is_empty() {
        format!("detected {profiles}")
    } else {
        format!(
            "detected {profiles}; configured roots exclude {}",
            layout.excluded_roots.join(", ")
        )
    };
    let next_action = if layout.excluded_roots.is_empty() {
        None
    } else {
        Some("add the excluded roots to `.synrepo/config.toml` and rerun reconcile".to_string())
    };

    ReadinessRow {
        capability: Capability::ProjectLayout,
        state: if layout.excluded_roots.is_empty() {
            ReadinessState::Supported
        } else {
            ReadinessState::Degraded
        },
        detail,
        next_action,
    }
}
