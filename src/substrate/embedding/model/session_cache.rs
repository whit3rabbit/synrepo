//! Process-global embedding session cache.
//!
//! Reusing initialized `EmbeddingSession` instances across task-route
//! classification, hybrid search, and explain triage avoids expensive
//! model re-commit and tokenizer parsing on every request.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::{EmbeddingSession, ModelResolution};
use crate::Result;

const MAX_SESSIONS: usize = 4;
const SESSION_IDLE_TTL: Duration = Duration::from_secs(30 * 60);

static SESSIONS: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();
static CONSTRUCTIONS: AtomicUsize = AtomicUsize::new(0);
static HITS: AtomicUsize = AtomicUsize::new(0);

struct CacheEntry {
    session: Arc<EmbeddingSession>,
    last_used: Instant,
}

/// Statistics for testing and diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SessionCacheStats {
    /// Number of fresh session constructions.
    pub constructions: usize,
    /// Number of cache hits.
    pub hits: usize,
}

/// Key derived from the full identity of a model resolution.
fn resolution_key(res: &ModelResolution) -> String {
    match res {
        ModelResolution::Onnx(onnx) => format!(
            "onnx:{}:{}:{}:{}",
            onnx.model_path.display(),
            onnx.tokenizer_path.display(),
            onnx.embedding_dim,
            onnx.query_prefix.as_deref().unwrap_or("")
        ),
        ModelResolution::Ollama(ollama) => format!(
            "ollama:{}:{}:{}:{}",
            ollama.endpoint,
            ollama.model_name,
            ollama.embedding_dim,
            ollama.query_prefix.as_deref().unwrap_or("")
        ),
    }
}

/// Evict idle entries based on TTL and LRU ordering.
fn evict_idle(cache: &mut HashMap<String, CacheEntry>) {
    let now = Instant::now();
    cache.retain(|_, entry| {
        now.duration_since(entry.last_used) < SESSION_IDLE_TTL
            || Arc::strong_count(&entry.session) > 1
    });

    if cache.len() <= MAX_SESSIONS {
        return;
    }

    let mut candidate_keys: Vec<(String, Instant, usize)> = cache
        .iter()
        .map(|(k, v)| (k.clone(), v.last_used, Arc::strong_count(&v.session)))
        .collect();

    // Prefer evicting entries that are not externally held (strong_count == 1),
    // sorted by oldest last_used first.
    candidate_keys.sort_by(|a, b| {
        let a_held = a.2 > 1;
        let b_held = b.2 > 1;
        a_held.cmp(&b_held).then_with(|| a.1.cmp(&b.1))
    });

    let remove_count = cache.len().saturating_sub(MAX_SESSIONS);
    for (key, _, _) in candidate_keys.into_iter().take(remove_count) {
        cache.remove(&key);
    }
}

/// Retrieve a shared session for `res` or construct and cache a new one.
pub fn shared_from_resolution(res: &ModelResolution) -> Result<Arc<EmbeddingSession>> {
    let key = resolution_key(res);
    let cache = SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));

    // 1. Check existing under lock
    {
        let mut map = cache
            .lock()
            .map_err(|_| crate::Error::Other(anyhow::anyhow!("session cache lock poisoned")))?;
        evict_idle(&mut map);
        if let Some(entry) = map.get_mut(&key) {
            entry.last_used = Instant::now();
            HITS.fetch_add(1, Ordering::Relaxed);
            return Ok(Arc::clone(&entry.session));
        }
    }

    // 2. Construct outside the lock (concurrent cold starts may duplicate once — acceptable)
    let new_session = Arc::new(EmbeddingSession::new_from_resolution(res)?);
    CONSTRUCTIONS.fetch_add(1, Ordering::Relaxed);

    // 3. Insert under lock
    let mut map = cache
        .lock()
        .map_err(|_| crate::Error::Other(anyhow::anyhow!("session cache lock poisoned")))?;
    let (session, was_present) = match map.get_mut(&key) {
        Some(entry) => {
            entry.last_used = Instant::now();
            (Arc::clone(&entry.session), true)
        }
        None => {
            let session = Arc::clone(&new_session);
            map.insert(
                key,
                CacheEntry {
                    session: new_session,
                    last_used: Instant::now(),
                },
            );
            (session, false)
        }
    };
    if !was_present {
        evict_idle(&mut map);
    }
    Ok(session)
}

#[doc(hidden)]
pub fn stats() -> SessionCacheStats {
    SessionCacheStats {
        constructions: CONSTRUCTIONS.load(Ordering::Relaxed),
        hits: HITS.load(Ordering::Relaxed),
    }
}

#[doc(hidden)]
pub fn reset_for_tests() {
    if let Some(lock) = SESSIONS.get() {
        if let Ok(mut map) = lock.lock() {
            map.clear();
        }
    }
    CONSTRUCTIONS.store(0, Ordering::Relaxed);
    HITS.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::substrate::embedding::model::OllamaModelResolution;

    fn sample_ollama(model: &str) -> ModelResolution {
        ModelResolution::Ollama(OllamaModelResolution {
            endpoint: "http://127.0.0.1:11434".to_string(),
            model_name: model.to_string(),
            embedding_dim: 384,
            normalize: true,
            batch_size: 32,
            query_prefix: None,
        })
    }

    #[test]
    fn repeated_shared_from_resolution_shares_session_and_counts_hits() {
        reset_for_tests();
        let res = sample_ollama("test-shared-model");
        let first = shared_from_resolution(&res).unwrap();
        let second = shared_from_resolution(&res).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        let s = stats();
        assert_eq!(s.constructions, 1);
        assert_eq!(s.hits, 1);
    }

    #[test]
    fn eviction_under_cap_drops_oldest_idle_entries() {
        reset_for_tests();
        let res1 = sample_ollama("model-1");
        let res2 = sample_ollama("model-2");
        let res3 = sample_ollama("model-3");
        let res4 = sample_ollama("model-4");
        let res5 = sample_ollama("model-5");

        let _s1 = shared_from_resolution(&res1).unwrap();
        let _s2 = shared_from_resolution(&res2).unwrap();
        let _s3 = shared_from_resolution(&res3).unwrap();
        let _s4 = shared_from_resolution(&res4).unwrap();

        // Dropping the handles so strong_count becomes 1 inside the cache
        drop(_s1);
        drop(_s2);
        drop(_s3);
        drop(_s4);

        // 5th insert should trigger eviction of oldest under cap = 4
        let _s5 = shared_from_resolution(&res5).unwrap();

        let cache = SESSIONS.get().unwrap().lock().unwrap();
        assert!(cache.len() <= MAX_SESSIONS);
    }
}
