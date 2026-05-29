//! Read-only status projection for configured branch-ref roots.

use std::{collections::BTreeMap, path::Path};

use serde::Serialize;

use crate::{
    config::Config,
    pipeline::{
        git::{branch_ref_heads, branch_root_id, BranchRefHead},
        watch::WatchServiceStatus,
    },
    substrate::{branch_index_exists, discover_roots, DiscoveryRootKind},
};

/// Summary of configured read-only branch-ref roots.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct BranchRootsStatus {
    pub(crate) configured: usize,
    pub(crate) resolved: usize,
    pub(crate) tracked: usize,
    pub(crate) stale: usize,
    pub(crate) missing: usize,
    pub(crate) poll_seconds: u32,
    pub(crate) monitored: bool,
    pub(crate) refs: Vec<BranchRefStatus>,
}

/// Per-ref branch status used by readiness/MCP detail output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct BranchRefStatus {
    pub(crate) ref_name: String,
    pub(crate) root_id: String,
    pub(crate) commit: Option<String>,
    pub(crate) resolved: bool,
    pub(crate) prepared: bool,
    pub(crate) indexed: bool,
    pub(crate) tracked: bool,
}

impl BranchRootsStatus {
    /// Inspect configured branch refs without mutating cache, graph, or indexes.
    pub(crate) fn inspect(
        repo_root: &Path,
        config: &Config,
        watch_status: Option<&WatchServiceStatus>,
    ) -> Self {
        if config.branch_roots.refs.is_empty() {
            return Self::empty(config.branch_roots.poll_seconds, false);
        }

        let monitored = matches!(watch_status, Some(WatchServiceStatus::Running(_)))
            && config.branch_roots.poll_seconds > 0;
        let heads = branch_ref_heads(repo_root, config)
            .into_iter()
            .map(|head| (head.root_id.clone(), head))
            .collect::<BTreeMap<_, _>>();
        let prepared = discover_roots(repo_root, config)
            .into_iter()
            .filter(|root| root.kind == DiscoveryRootKind::BranchRef)
            .map(|root| (root.discriminant, root.commit))
            .collect::<BTreeMap<_, _>>();

        let refs = config
            .branch_roots
            .refs
            .iter()
            .map(|ref_name| {
                let root_id = branch_root_id(ref_name);
                let head = heads.get(&root_id);
                ref_status(repo_root, ref_name, root_id, head, &prepared)
            })
            .collect::<Vec<_>>();

        Self::from_refs(config.branch_roots.poll_seconds, monitored, refs)
    }

    pub(crate) fn compact_label(&self) -> String {
        if self.configured == 0 {
            return "off".to_string();
        }
        if self.missing > 0 {
            return format!("{}/{} missing", self.tracked, self.configured);
        }
        if self.stale > 0 {
            return format!("{}/{} stale", self.tracked, self.configured);
        }
        if self.monitored {
            return format!(
                "{}/{} @{}s",
                self.tracked, self.configured, self.poll_seconds
            );
        }
        format!("{}/{} idle", self.tracked, self.configured)
    }

    pub(crate) fn empty(poll_seconds: u32, monitored: bool) -> Self {
        Self::from_refs(poll_seconds, monitored, Vec::new())
    }

    pub(crate) fn from_refs(
        poll_seconds: u32,
        monitored: bool,
        refs: Vec<BranchRefStatus>,
    ) -> Self {
        let configured = refs.len();
        let resolved = refs.iter().filter(|r| r.resolved).count();
        let tracked = refs.iter().filter(|r| r.tracked).count();
        let missing = configured.saturating_sub(resolved);
        let stale = refs.iter().filter(|r| r.resolved && !r.tracked).count();
        Self {
            configured,
            resolved,
            tracked,
            stale,
            missing,
            poll_seconds,
            monitored: monitored && configured > 0,
            refs,
        }
    }
}

impl BranchRefStatus {
    #[cfg(test)]
    pub(crate) fn tracked(ref_name: &str) -> Self {
        let root_id = branch_root_id(ref_name);
        Self {
            ref_name: ref_name.to_string(),
            root_id,
            commit: Some("abc123".to_string()),
            resolved: true,
            prepared: true,
            indexed: true,
            tracked: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn stale(ref_name: &str) -> Self {
        let mut status = Self::tracked(ref_name);
        status.indexed = false;
        status.tracked = false;
        status
    }

    #[cfg(test)]
    pub(crate) fn missing(ref_name: &str) -> Self {
        let root_id = branch_root_id(ref_name);
        Self {
            ref_name: ref_name.to_string(),
            root_id,
            commit: None,
            resolved: false,
            prepared: false,
            indexed: false,
            tracked: false,
        }
    }
}

fn ref_status(
    repo_root: &Path,
    ref_name: &str,
    root_id: String,
    head: Option<&BranchRefHead>,
    prepared: &BTreeMap<String, Option<String>>,
) -> BranchRefStatus {
    let commit = head.map(|head| head.commit.clone());
    let prepared = head.is_some_and(|head| {
        prepared
            .get(&root_id)
            .and_then(|commit| commit.as_deref())
            .is_some_and(|commit| commit == head.commit)
    });
    let indexed = branch_index_exists(repo_root, &root_id);
    let tracked = head.is_some() && prepared && indexed;
    BranchRefStatus {
        ref_name: ref_name.to_string(),
        root_id,
        commit,
        resolved: head.is_some(),
        prepared,
        indexed,
        tracked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BranchRootsConfig;

    #[test]
    fn compact_label_reports_configured_state() {
        assert_eq!(BranchRootsStatus::empty(30, false).compact_label(), "off");

        let tracked = BranchRootsStatus::from_refs(
            30,
            true,
            vec![
                BranchRefStatus::tracked("refs/heads/main"),
                BranchRefStatus::tracked("refs/heads/release"),
            ],
        );
        assert_eq!(tracked.compact_label(), "2/2 @30s");

        let idle = BranchRootsStatus::from_refs(
            30,
            false,
            vec![BranchRefStatus::tracked("refs/heads/main")],
        );
        assert_eq!(idle.compact_label(), "1/1 idle");

        let stale = BranchRootsStatus::from_refs(
            30,
            false,
            vec![
                BranchRefStatus::tracked("refs/heads/main"),
                BranchRefStatus::stale("refs/heads/release"),
            ],
        );
        assert_eq!(stale.compact_label(), "1/2 stale");

        let missing = BranchRootsStatus::from_refs(
            30,
            true,
            vec![
                BranchRefStatus::tracked("refs/heads/main"),
                BranchRefStatus::missing("refs/heads/release"),
            ],
        );
        assert_eq!(missing.compact_label(), "1/2 missing");
    }

    #[test]
    fn inspect_reports_missing_refs_without_git_repository() {
        let repo = tempfile::tempdir().unwrap();
        let config = Config {
            branch_roots: BranchRootsConfig {
                refs: vec!["refs/heads/main".to_string()],
                poll_seconds: 30,
            },
            ..Config::default()
        };

        let status = BranchRootsStatus::inspect(repo.path(), &config, None);

        assert_eq!(status.configured, 1);
        assert_eq!(status.resolved, 0);
        assert_eq!(status.missing, 1);
        assert_eq!(status.compact_label(), "0/1 missing");
    }
}
