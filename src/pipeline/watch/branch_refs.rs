use std::{collections::BTreeMap, path::Path, time::Instant};

use crate::config::Config;

pub(super) struct BranchRefPoller {
    last_heads: BTreeMap<String, String>,
    last_poll: Instant,
}

impl BranchRefPoller {
    pub(super) fn new(repo_root: &Path, config: &Config) -> Self {
        Self {
            last_heads: crate::pipeline::git::head_map(repo_root, config),
            last_poll: Instant::now(),
        }
    }

    pub(super) fn maybe_changed(&mut self, repo_root: &Path, config: &Config) -> bool {
        if config.branch_roots.refs.is_empty() || config.branch_roots.poll_seconds == 0 {
            return false;
        }
        if self.last_poll.elapsed().as_secs() < config.branch_roots.poll_seconds as u64 {
            return false;
        }
        self.last_poll = Instant::now();
        let current = crate::pipeline::git::head_map(repo_root, config);
        if current == self.last_heads {
            return false;
        }
        self.last_heads = current;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_never_changes() {
        let repo = tempfile::tempdir().unwrap();
        let mut poller = BranchRefPoller::new(repo.path(), &Config::default());
        assert!(!poller.maybe_changed(repo.path(), &Config::default()));
    }
}
