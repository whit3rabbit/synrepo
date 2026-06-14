use std::time::Duration;

use notify_debouncer_full::{notify::Error, DebounceEventResult};

use super::super::debouncer::{new_watch_debouncer, WatchDebouncer};

#[test]
fn watch_debouncer_constructor_uses_no_cache_type() {
    let _typed: fn(
        Duration,
        Option<Duration>,
        fn(DebounceEventResult),
    ) -> Result<WatchDebouncer, Error> = new_watch_debouncer::<fn(DebounceEventResult)>;
}
