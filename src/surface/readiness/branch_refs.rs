use crate::{
    config::Config,
    surface::{branch_refs::BranchRootsStatus, status_snapshot::StatusSnapshot},
};

use super::{Capability, ReadinessRow, ReadinessState};

pub(super) fn branch_refs_row(
    repo_root: &std::path::Path,
    config: &Config,
    snapshot: &StatusSnapshot,
) -> ReadinessRow {
    let watch_status = snapshot.diagnostics.as_ref().map(|diag| &diag.watch_status);
    let status = BranchRootsStatus::inspect(repo_root, config, watch_status);
    branch_refs_row_from_status(&status)
}

pub(crate) fn branch_refs_row_from_status(status: &BranchRootsStatus) -> ReadinessRow {
    if status.configured == 0 {
        return ReadinessRow {
            capability: Capability::BranchRefs,
            state: ReadinessState::Disabled,
            detail: "no branch refs configured".to_string(),
            next_action: None,
        };
    }

    if status.missing > 0 {
        return ReadinessRow {
            capability: Capability::BranchRefs,
            state: ReadinessState::Degraded,
            detail: format!(
                "{}/{} tracked; {} missing locally",
                status.tracked, status.configured, status.missing
            ),
            next_action: Some(
                "fetch missing refs or update branch_roots.refs, then run `synrepo reconcile`"
                    .to_string(),
            ),
        };
    }

    if status.stale > 0 {
        return ReadinessRow {
            capability: Capability::BranchRefs,
            state: ReadinessState::Stale,
            detail: format!(
                "{}/{} tracked; {} need snapshot/index refresh",
                status.tracked, status.configured, status.stale
            ),
            next_action: Some("run `synrepo reconcile`".to_string()),
        };
    }

    let monitoring = if status.monitored {
        format!("monitored @{}s", status.poll_seconds)
    } else if status.poll_seconds == 0 {
        "polling disabled".to_string()
    } else {
        "not monitored".to_string()
    };

    ReadinessRow {
        capability: Capability::BranchRefs,
        state: ReadinessState::Supported,
        detail: format!(
            "{}/{} tracked; {monitoring}",
            status.tracked, status.configured
        ),
        next_action: None,
    }
}
