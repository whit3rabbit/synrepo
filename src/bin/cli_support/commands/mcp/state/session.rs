use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

#[derive(Clone, Default)]
pub(crate) struct SessionState {
    inner: std::sync::Arc<Mutex<SessionInner>>,
}

#[derive(Default)]
struct SessionInner {
    metrics: SessionMetrics,
    buckets: HashMap<&'static str, TokenBucket>,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub(crate) struct SessionMetrics {
    pub(crate) calls_total: u64,
    pub(crate) errors_total: u64,
    pub(crate) rate_limited_total: u64,
    pub(crate) calls_by_tool: HashMap<String, u64>,
    pub(crate) errors_by_tool: HashMap<String, u64>,
}

struct TokenBucket {
    window_start: Instant,
    count: u32,
    limit: u32,
    window: Duration,
}

impl SessionState {
    pub(crate) fn check_rate_limit(&self, tool: &'static str) -> anyhow::Result<()> {
        let mut inner = self.inner.lock();
        let (limit, window) = rate_limit_for(tool);
        let bucket = inner.buckets.entry(tool).or_insert_with(|| TokenBucket {
            window_start: Instant::now(),
            count: 0,
            limit,
            window,
        });
        let now = Instant::now();
        if now.duration_since(bucket.window_start) >= bucket.window {
            bucket.window_start = now;
            bucket.count = 0;
        }
        bucket.limit = limit;
        bucket.window = window;
        if bucket.count >= bucket.limit {
            inner.metrics.rate_limited_total += 1;
            return Err(synrepo::surface::mcp::error::McpError::new(
                synrepo::surface::mcp::error::ErrorCode::RateLimited,
                format!("rate limit exceeded for {tool}"),
            )
            .into());
        }
        bucket.count += 1;
        Ok(())
    }

    pub(crate) fn record_tool(&self, tool: &str, errored: bool) {
        let mut inner = self.inner.lock();
        inner.metrics.calls_total += 1;
        *inner
            .metrics
            .calls_by_tool
            .entry(tool.to_string())
            .or_default() += 1;
        if errored {
            inner.metrics.errors_total += 1;
            *inner
                .metrics
                .errors_by_tool
                .entry(tool.to_string())
                .or_default() += 1;
        }
    }

    pub(crate) fn snapshot(&self) -> SessionMetrics {
        self.inner.lock().metrics.clone()
    }
}

fn rate_limit_for(tool: &str) -> (u32, Duration) {
    if tool == "synrepo_refresh_commentary" {
        (3, Duration::from_secs(60))
    } else if tool == "synrepo_card" || tool == "synrepo_explain" || tool == "synrepo_context_pack"
    {
        (10, Duration::from_secs(1))
    } else {
        (30, Duration::from_secs(1))
    }
}
