use std::time::Duration;

use notify_debouncer_full::{
    new_debouncer_opt,
    notify::{Config as NotifyConfig, Error, RecommendedWatcher},
    DebounceEventHandler, Debouncer, NoCache,
};

pub(super) type WatchDebouncer = Debouncer<RecommendedWatcher, NoCache>;

pub(super) fn new_watch_debouncer<F>(
    timeout: Duration,
    tick_rate: Option<Duration>,
    event_handler: F,
) -> Result<WatchDebouncer, Error>
where
    F: DebounceEventHandler,
{
    new_debouncer_opt::<F, RecommendedWatcher, NoCache>(
        timeout,
        tick_rate,
        event_handler,
        NoCache::new(),
        NotifyConfig::default(),
    )
}
