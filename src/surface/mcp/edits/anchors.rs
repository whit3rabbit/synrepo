use std::{
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use serde::Serialize;

#[derive(Clone, Debug, Eq)]
struct AnchorKey {
    repo_root: PathBuf,
    task_id: String,
    path: String,
    content_hash: String,
    anchor_state_version: String,
}

impl PartialEq for AnchorKey {
    fn eq(&self, other: &Self) -> bool {
        self.repo_root == other.repo_root
            && self.task_id == other.task_id
            && self.path == other.path
            && self.content_hash == other.content_hash
            && self.anchor_state_version == other.anchor_state_version
    }
}

impl Hash for AnchorKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.repo_root.hash(state);
        self.task_id.hash(state);
        self.path.hash(state);
        self.content_hash.hash(state);
        self.anchor_state_version.hash(state);
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AnchorLine {
    pub anchor: String,
    pub line: usize,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct PreparedAnchorState {
    pub repo_root: PathBuf,
    pub task_id: String,
    pub path: String,
    pub content_hash: String,
    pub anchor_state_version: String,
    pub anchors: Vec<AnchorLine>,
}

#[derive(Clone, Debug)]
struct CacheEntry {
    state: PreparedAnchorState,
    expires_at: Instant,
}

#[derive(Debug)]
pub struct AnchorManager {
    cache: Mutex<CacheState>,
    ttl: Duration,
    max_entries: usize,
    next_version: AtomicU64,
}

#[derive(Debug, Default)]
struct CacheState {
    entries: HashMap<AnchorKey, CacheEntry>,
    order: VecDeque<AnchorKey>,
}

impl Default for AnchorManager {
    fn default() -> Self {
        Self {
            cache: Mutex::new(CacheState::default()),
            ttl: Duration::from_secs(30 * 60),
            max_entries: 128,
            next_version: AtomicU64::new(1),
        }
    }
}

impl AnchorManager {
    pub fn next_version(
        &self,
        repo_root: &std::path::Path,
        path: &str,
        content_hash: &str,
    ) -> String {
        let counter = self.next_version.fetch_add(1, Ordering::Relaxed);
        let seed = format!("{}:{path}:{content_hash}:{counter}", repo_root.display());
        let digest = blake3::hash(seed.as_bytes()).to_hex().to_string();
        format!("asv-{}", &digest[..16])
    }

    pub fn insert(&self, state: PreparedAnchorState) {
        let now = Instant::now();
        let key = key_for(&state);
        let entry = CacheEntry {
            state,
            expires_at: now + self.ttl,
        };
        let mut cache = self.cache.lock();
        self.prune_locked(&mut cache, now);
        cache.entries.insert(key.clone(), entry);
        cache.order.push_back(key);
        self.enforce_lru_locked(&mut cache);
    }

    pub fn get(
        &self,
        repo_root: &std::path::Path,
        task_id: &str,
        path: &str,
        content_hash: &str,
        anchor_state_version: &str,
    ) -> Option<PreparedAnchorState> {
        let now = Instant::now();
        let key = AnchorKey {
            repo_root: repo_root.to_path_buf(),
            task_id: task_id.to_string(),
            path: path.to_string(),
            content_hash: content_hash.to_string(),
            anchor_state_version: anchor_state_version.to_string(),
        };
        let mut cache = self.cache.lock();
        self.prune_locked(&mut cache, now);
        cache.entries.get(&key).map(|entry| entry.state.clone())
    }

    fn prune_locked(&self, cache: &mut CacheState, now: Instant) {
        cache.entries.retain(|_, entry| entry.expires_at > now);
        cache.order.retain(|key| cache.entries.contains_key(key));
    }

    fn enforce_lru_locked(&self, cache: &mut CacheState) {
        while cache.entries.len() > self.max_entries {
            let Some(oldest) = cache.order.pop_front() else {
                return;
            };
            cache.entries.remove(&oldest);
        }
    }
}

fn key_for(state: &PreparedAnchorState) -> AnchorKey {
    AnchorKey {
        repo_root: state.repo_root.clone(),
        task_id: state.task_id.clone(),
        path: state.path.clone(),
        content_hash: state.content_hash.clone(),
        anchor_state_version: state.anchor_state_version.clone(),
    }
}

pub fn anchor_manager() -> &'static AnchorManager {
    static MANAGER: std::sync::OnceLock<AnchorManager> = std::sync::OnceLock::new();
    MANAGER.get_or_init(AnchorManager::default)
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{atomic::AtomicU64, Arc},
        thread,
        time::Duration,
    };

    use super::{AnchorLine, AnchorManager, CacheState, PreparedAnchorState};
    use parking_lot::Mutex;

    fn manager(max_entries: usize) -> AnchorManager {
        AnchorManager {
            cache: Mutex::new(CacheState::default()),
            ttl: Duration::from_secs(60),
            max_entries,
            next_version: AtomicU64::new(1),
        }
    }

    fn state(index: usize) -> PreparedAnchorState {
        PreparedAnchorState {
            repo_root: PathBuf::from("/repo"),
            task_id: format!("task-{index}"),
            path: "src/lib.rs".to_string(),
            content_hash: "hash".to_string(),
            anchor_state_version: format!("asv-{index}"),
            anchors: vec![AnchorLine {
                anchor: "L000001".to_string(),
                line: 1,
                text: "line".to_string(),
            }],
        }
    }

    #[test]
    fn lru_eviction_keeps_entries_and_order_consistent() {
        let manager = manager(2);
        manager.insert(state(1));
        manager.insert(state(2));
        manager.insert(state(3));

        assert!(manager
            .get(
                PathBuf::from("/repo").as_path(),
                "task-1",
                "src/lib.rs",
                "hash",
                "asv-1"
            )
            .is_none());
        assert!(manager
            .get(
                PathBuf::from("/repo").as_path(),
                "task-2",
                "src/lib.rs",
                "hash",
                "asv-2"
            )
            .is_some());
        assert!(manager
            .get(
                PathBuf::from("/repo").as_path(),
                "task-3",
                "src/lib.rs",
                "hash",
                "asv-3"
            )
            .is_some());

        let cache = manager.cache.lock();
        assert_eq!(cache.entries.len(), 2);
        assert!(cache
            .order
            .iter()
            .all(|key| cache.entries.contains_key(key)));
    }

    #[test]
    fn concurrent_inserts_preserve_lru_invariants() {
        let manager = Arc::new(manager(8));
        let mut threads = Vec::new();
        for index in 0..32 {
            let manager = Arc::clone(&manager);
            threads.push(thread::spawn(move || manager.insert(state(index))));
        }
        for thread in threads {
            thread.join().unwrap();
        }

        let cache = manager.cache.lock();
        assert!(cache.entries.len() <= 8);
        assert!(cache.order.len() <= 8);
        assert!(cache
            .order
            .iter()
            .all(|key| cache.entries.contains_key(key)));
    }
}
