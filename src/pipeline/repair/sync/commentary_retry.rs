//! Shared commentary rate-limit retry policy.

use std::time::Duration;

use backon::{BackoffBuilder, ExponentialBackoff, ExponentialBuilder};

use crate::pipeline::explain::{CommentarySkip, CommentarySkipReason};

pub(super) const MAX_RATE_LIMIT_ATTEMPTS: usize = 3;
pub(super) const DEFAULT_RATE_LIMIT_BACKOFF: Duration = Duration::from_millis(750);
pub(super) const MAX_RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(5);

pub(super) enum RetryAction {
    Retry { delay: Duration },
    Stop { queued_for_next_run: bool },
}

pub(super) struct CommentaryRetry {
    retry_attempts: usize,
    delays: ExponentialBackoff,
}

impl CommentaryRetry {
    pub(super) fn new() -> Self {
        Self {
            retry_attempts: 0,
            delays: ExponentialBuilder::new()
                .with_min_delay(DEFAULT_RATE_LIMIT_BACKOFF)
                .with_max_delay(MAX_RATE_LIMIT_BACKOFF)
                .with_max_times(MAX_RATE_LIMIT_ATTEMPTS.saturating_sub(1))
                .build(),
        }
    }

    pub(super) fn retry_attempts(&self) -> usize {
        self.retry_attempts
    }

    pub(super) fn next_action(&mut self, skip: &CommentarySkip) -> RetryAction {
        if skip.reason != CommentarySkipReason::RateLimited {
            return RetryAction::Stop {
                queued_for_next_run: false,
            };
        }

        if skip
            .retry_after
            .is_some_and(|delay| delay > MAX_RATE_LIMIT_BACKOFF)
        {
            return RetryAction::Stop {
                queued_for_next_run: true,
            };
        }

        let Some(backoff_delay) = self.delays.next() else {
            return RetryAction::Stop {
                queued_for_next_run: true,
            };
        };

        self.retry_attempts += 1;
        RetryAction::Retry {
            delay: skip.retry_after.unwrap_or(backoff_delay),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_under_cap_overrides_backoff_delay() {
        let mut retry = CommentaryRetry::new();
        let skip = CommentarySkip::rate_limited("limited", Some(Duration::from_millis(25)));

        match retry.next_action(&skip) {
            RetryAction::Retry { delay } => assert_eq!(delay, Duration::from_millis(25)),
            RetryAction::Stop { .. } => panic!("expected retry"),
        }
        assert_eq!(retry.retry_attempts(), 1);
    }

    #[test]
    fn retry_after_over_cap_queues_without_sleeping() {
        let mut retry = CommentaryRetry::new();
        let skip = CommentarySkip::rate_limited("limited", Some(Duration::from_secs(30)));

        match retry.next_action(&skip) {
            RetryAction::Stop {
                queued_for_next_run,
            } => assert!(queued_for_next_run),
            RetryAction::Retry { .. } => panic!("expected stop"),
        }
        assert_eq!(retry.retry_attempts(), 0);
    }

    #[test]
    fn exponential_policy_allows_two_retries_for_three_total_attempts() {
        let mut retry = CommentaryRetry::new();
        let skip = CommentarySkip::rate_limited("limited", None);

        assert!(matches!(
            retry.next_action(&skip),
            RetryAction::Retry { .. }
        ));
        assert!(matches!(
            retry.next_action(&skip),
            RetryAction::Retry { .. }
        ));
        assert!(matches!(
            retry.next_action(&skip),
            RetryAction::Stop {
                queued_for_next_run: true
            }
        ));
        assert_eq!(retry.retry_attempts(), MAX_RATE_LIMIT_ATTEMPTS - 1);
    }

    #[test]
    fn non_rate_limit_skip_is_not_retried() {
        let mut retry = CommentaryRetry::new();
        let skip = CommentarySkip::new(CommentarySkipReason::ProviderFailed);

        assert!(matches!(
            retry.next_action(&skip),
            RetryAction::Stop {
                queued_for_next_run: false
            }
        ));
        assert_eq!(retry.retry_attempts(), 0);
    }
}
