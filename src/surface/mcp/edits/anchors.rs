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
    entries: Mutex<HashMap<AnchorKey, CacheEntry>>,
    order: Mutex<VecDeque<AnchorKey>>,
    ttl: Duration,
    max_entries: usize,
    next_version: AtomicU64,
}

impl Default for AnchorManager {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            order: Mutex::new(VecDeque::new()),
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
        self.prune();
        let key = key_for(&state);
        let entry = CacheEntry {
            state,
            expires_at: Instant::now() + self.ttl,
        };
        self.entries.lock().insert(key.clone(), entry);
        self.order.lock().push_back(key);
        self.enforce_lru();
    }

    pub fn get(
        &self,
        repo_root: &std::path::Path,
        task_id: &str,
        path: &str,
        content_hash: &str,
        anchor_state_version: &str,
    ) -> Option<PreparedAnchorState> {
        self.prune();
        let key = AnchorKey {
            repo_root: repo_root.to_path_buf(),
            task_id: task_id.to_string(),
            path: path.to_string(),
            content_hash: content_hash.to_string(),
            anchor_state_version: anchor_state_version.to_string(),
        };
        self.entries
            .lock()
            .get(&key)
            .map(|entry| entry.state.clone())
    }

    fn prune(&self) {
        let now = Instant::now();
        self.entries
            .lock()
            .retain(|_, entry| entry.expires_at > now);
        self.order
            .lock()
            .retain(|key| self.entries.lock().contains_key(key));
    }

    fn enforce_lru(&self) {
        loop {
            if self.entries.lock().len() <= self.max_entries {
                break;
            }
            let Some(oldest) = self.order.lock().pop_front() else {
                break;
            };
            self.entries.lock().remove(&oldest);
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
