//! Handoffs types and data structures.

use serde::{Deserialize, Serialize};

/// Source of the handoff item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffSource {
    /// Repair surface finding from sync operations.
    Repair,
    /// Cross-link candidate from overlay.
    CrossLink,
    /// Git hotspot signal from frequent commits.
    Hotspot,
}

/// Priority level for handoff items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandoffPriority {
    /// Low priority; informational.
    Low,
    /// Medium priority; address when convenient.
    Medium,
    /// High priority; should be addressed soon.
    High,
    /// Highest priority; requires immediate attention.
    Critical,
}

impl HandoffPriority {
    /// Returns the display name for the priority.
    pub fn as_str(&self) -> &'static str {
        match self {
            HandoffPriority::Critical => "critical",
            HandoffPriority::High => "high",
            HandoffPriority::Medium => "medium",
            HandoffPriority::Low => "low",
        }
    }
}

/// A single handoff item from any source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffItem {
    /// Unique identifier for this item.
    pub id: String,
    /// Type of the handoff item.
    #[serde(rename = "type")]
    pub item_type: HandoffSource,
    /// File path or symbol reference.
    pub source: String,
    /// Actionable recommendation text.
    pub recommendation: String,
    /// Priority level.
    pub priority: HandoffPriority,
    /// File where the item originates.
    pub source_file: String,
    /// Line number in the source file (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_line: Option<u32>,
}

impl HandoffItem {
    /// Create a new handoff item.
    pub fn new(
        id: String,
        item_type: HandoffSource,
        source: String,
        recommendation: String,
        priority: HandoffPriority,
        source_file: String,
        source_line: Option<u32>,
    ) -> Self {
        Self {
            id,
            item_type,
            source,
            recommendation,
            priority,
            source_file,
            source_line,
        }
    }
}

/// Request parameters for handoffs query.
#[derive(Debug, Clone)]
pub struct HandoffsRequest {
    /// Maximum number of items to return.
    pub limit: usize,
    /// Only include items from the last N days.
    pub since_days: u32,
}

impl Default for HandoffsRequest {
    fn default() -> Self {
        Self {
            limit: 20,
            since_days: 30,
        }
    }
}

impl HandoffsRequest {
    /// Create a new request with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a request with custom limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Create a request with custom since_days.
    pub fn with_since_days(mut self, since_days: u32) -> Self {
        self.since_days = since_days;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handoff_item_creation() {
        let item = HandoffItem::new(
            "test-1".to_string(),
            HandoffSource::Repair,
            "src/main.rs".to_string(),
            "Consider adding an index for better query performance".to_string(),
            HandoffPriority::High,
            "src/main.rs".to_string(),
            Some(42),
        );
        assert_eq!(item.id, "test-1");
        assert_eq!(item.item_type, HandoffSource::Repair);
        assert_eq!(item.source, "src/main.rs");
        assert_eq!(item.priority, HandoffPriority::High);
    }

    #[test]
    fn test_handoff_priority_ordering() {
        assert!(HandoffPriority::Critical > HandoffPriority::High);
        assert!(HandoffPriority::High > HandoffPriority::Medium);
        assert!(HandoffPriority::Medium > HandoffPriority::Low);
    }

    #[test]
    fn test_handoffs_request_defaults() {
        let req = HandoffsRequest::new();
        assert_eq!(req.limit, 20);
        assert_eq!(req.since_days, 30);
    }

    #[test]
    fn test_handoffs_request_builder() {
        let req = HandoffsRequest::new().with_limit(10).with_since_days(7);
        assert_eq!(req.limit, 10);
        assert_eq!(req.since_days, 7);
    }
}
