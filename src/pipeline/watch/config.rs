use std::time::Duration;

/// Configuration for the watch and reconcile loop.
#[derive(Clone, Debug)]
pub struct WatchConfig {
    /// How long after the last filesystem event to wait before triggering a
    /// reconcile pass.
    pub debounce_timeout: Duration,
    /// Upper bound on events logged per reconcile cycle.
    pub max_events_per_cycle: usize,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            debounce_timeout: Duration::from_millis(500),
            max_events_per_cycle: 1000,
        }
    }
}
