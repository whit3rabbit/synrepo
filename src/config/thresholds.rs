//! Confidence-tier partition thresholds for cross-link classification.

use serde::{Deserialize, Serialize};

/// TOML-friendly mirror of `overlay::ConfidenceThresholds`. Lives in this
/// module so config loading does not pull the overlay types into the config
/// layer; `From` conversions in both directions keep the two in sync.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CrossLinkConfidenceThresholds {
    /// Scores at or above this value classify as `High`.
    #[serde(default = "default_high_threshold")]
    pub high: f32,
    /// Scores at or above this value (and below `high`) classify as
    /// `ReviewQueue`; anything lower is `BelowThreshold`.
    #[serde(default = "default_review_queue_threshold")]
    pub review_queue: f32,
}

impl Default for CrossLinkConfidenceThresholds {
    fn default() -> Self {
        Self {
            high: default_high_threshold(),
            review_queue: default_review_queue_threshold(),
        }
    }
}

impl From<CrossLinkConfidenceThresholds> for crate::overlay::ConfidenceThresholds {
    fn from(c: CrossLinkConfidenceThresholds) -> Self {
        crate::overlay::ConfidenceThresholds {
            high: c.high,
            review_queue: c.review_queue,
        }
    }
}

impl From<crate::overlay::ConfidenceThresholds> for CrossLinkConfidenceThresholds {
    fn from(c: crate::overlay::ConfidenceThresholds) -> Self {
        CrossLinkConfidenceThresholds {
            high: c.high,
            review_queue: c.review_queue,
        }
    }
}

fn default_high_threshold() -> f32 {
    0.85
}

fn default_review_queue_threshold() -> f32 {
    0.6
}
