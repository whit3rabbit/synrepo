use std::time::Duration;

use crate::pipeline::{
    repair::SyncOptions,
    watch::{control::DEFAULT_CONTROL_TIMEOUT, WatchControlRequest},
};

#[test]
fn timeout_policy_is_selected_from_request_kind() {
    for request in [
        WatchControlRequest::Status,
        WatchControlRequest::Stop,
        WatchControlRequest::SuppressPaths {
            paths: Vec::new(),
            ttl_ms: 1,
        },
        WatchControlRequest::SetAutoSync { enabled: true },
    ] {
        assert_eq!(request.client_timeout(), Some(DEFAULT_CONTROL_TIMEOUT));
    }

    for request in [
        WatchControlRequest::ReconcileNow { fast: false },
        WatchControlRequest::SyncNow {
            options: SyncOptions::default(),
        },
        WatchControlRequest::EmbeddingsBuildNow,
    ] {
        assert_eq!(request.client_timeout(), None);
    }

    assert_eq!(DEFAULT_CONTROL_TIMEOUT, Duration::from_secs(5));
}
