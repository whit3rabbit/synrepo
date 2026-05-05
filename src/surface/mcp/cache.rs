use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use crate::surface::card::compiler::GraphCardCompiler;

const MAX_CONCURRENT_READS_PER_REPO: usize = 4;
const READ_LIMIT_WAIT: Duration = Duration::from_millis(250);
const MAX_POOLED_SQLITE_COMPILERS_PER_REPO: usize = 4;
const MAX_TRACKED_REPOS: usize = 128;
const REPO_CACHE_IDLE_TTL: Duration = Duration::from_secs(30 * 60);

static REPO_CACHE: OnceLock<StdMutex<HashMap<PathBuf, RepoCacheEntry>>> = OnceLock::new();

struct RepoCacheEntry {
    limiter: Arc<ReadLimiter>,
    compilers: Vec<GraphCardCompiler>,
    last_used: Instant,
}

pub(super) struct ReadPermit {
    limiter: Arc<ReadLimiter>,
}

pub(super) fn acquire_read(repo_root: &Path) -> crate::Result<ReadPermit> {
    let limiter = {
        let cache = REPO_CACHE.get_or_init(|| StdMutex::new(HashMap::new()));
        let mut cache = cache
            .lock()
            .map_err(|_| crate::Error::Other(anyhow::anyhow!("MCP repo cache lock poisoned")))?;
        let entry = touch_entry(&mut cache, repo_root);
        Arc::clone(&entry.limiter)
    };
    limiter.acquire()
}

pub(super) fn take_compiler(repo_root: &Path) -> Option<crate::Result<GraphCardCompiler>> {
    let cache = REPO_CACHE.get_or_init(|| StdMutex::new(HashMap::new()));
    let mut cache = cache.lock().ok()?;
    let entry = touch_entry(&mut cache, repo_root);
    entry.compilers.pop().map(Ok)
}

pub(super) fn return_compiler(repo_root: &Path, compiler: GraphCardCompiler) {
    let cache = REPO_CACHE.get_or_init(|| StdMutex::new(HashMap::new()));
    if let Ok(mut cache) = cache.lock() {
        let entry = touch_entry(&mut cache, repo_root);
        if entry.compilers.len() < MAX_POOLED_SQLITE_COMPILERS_PER_REPO {
            entry.compilers.push(compiler);
        }
    }
}

fn touch_entry<'a>(
    cache: &'a mut HashMap<PathBuf, RepoCacheEntry>,
    repo_root: &Path,
) -> &'a mut RepoCacheEntry {
    evict_idle(cache);
    let entry = cache
        .entry(repo_root.to_path_buf())
        .or_insert_with(RepoCacheEntry::new);
    entry.last_used = Instant::now();
    entry
}

fn evict_idle(cache: &mut HashMap<PathBuf, RepoCacheEntry>) {
    let now = Instant::now();
    cache.retain(|_, entry| {
        now.duration_since(entry.last_used) < REPO_CACHE_IDLE_TTL || !entry.limiter.is_idle()
    });

    if cache.len() <= MAX_TRACKED_REPOS {
        return;
    }

    let mut idle_keys = cache
        .iter()
        .filter(|(_, entry)| entry.limiter.is_idle())
        .map(|(path, entry)| (path.clone(), entry.last_used))
        .collect::<Vec<_>>();
    idle_keys.sort_by_key(|(_, last_used)| *last_used);
    let remove_count = cache.len().saturating_sub(MAX_TRACKED_REPOS);
    for (path, _) in idle_keys.into_iter().take(remove_count) {
        cache.remove(&path);
    }
}

impl RepoCacheEntry {
    fn new() -> Self {
        Self {
            limiter: Arc::new(ReadLimiter {
                state: StdMutex::new(ReadLimiterState::default()),
                cvar: Condvar::new(),
            }),
            compilers: Vec::new(),
            last_used: Instant::now(),
        }
    }
}

struct ReadLimiter {
    state: StdMutex<ReadLimiterState>,
    cvar: Condvar,
}

#[derive(Default)]
struct ReadLimiterState {
    active: usize,
}

impl ReadLimiter {
    fn acquire(self: &Arc<Self>) -> crate::Result<ReadPermit> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| crate::Error::Other(anyhow::anyhow!("MCP read limiter lock poisoned")))?;
        while state.active >= MAX_CONCURRENT_READS_PER_REPO {
            let (next, timeout) = self
                .cvar
                .wait_timeout(state, READ_LIMIT_WAIT)
                .map_err(|_| {
                    crate::Error::Other(anyhow::anyhow!("MCP read limiter lock poisoned"))
                })?;
            state = next;
            if timeout.timed_out() && state.active >= MAX_CONCURRENT_READS_PER_REPO {
                return Err(crate::Error::Other(
                    super::error::McpError::busy("too many concurrent MCP read snapshots").into(),
                ));
            }
        }
        state.active += 1;
        Ok(ReadPermit {
            limiter: Arc::clone(self),
        })
    }

    fn is_idle(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.active == 0)
            .unwrap_or(false)
    }
}

impl Drop for ReadPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.limiter.state.lock() {
            state.active = state.active.saturating_sub(1);
            self.limiter.cvar.notify_one();
        }
    }
}
