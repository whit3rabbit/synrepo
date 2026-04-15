## ADDED Requirements

### Requirement: Define repair-log rotation as a compact sub-action
synrepo SHALL include repair-log rotation as a compact sub-action. Rotation SHALL summarize entries older than the policy's retention window into a single header line and rewrite the JSONL file atomically.

#### Scenario: Detect compactable repair-log entries
- **WHEN** a compact dry-run evaluates the repair-log
- **THEN** the planner reports the count of entries older than the policy's retention window
- **AND** the planner reports whether a WAL checkpoint and index rebuild are warranted

#### Scenario: Execute repair-log rotation
- **WHEN** a compact pass executes repair-log rotation
- **THEN** entries older than the retention window are summarized into a single JSON header line containing counts grouped by surface and action
- **AND** entries within the retention window are preserved in chronological order
- **AND** the file is rewritten atomically (write to temp file, then rename)

#### Scenario: Repair-log rotation is idempotent
- **WHEN** a compact pass runs on a repair-log that was already compacted
- **THEN** the rotation is a no-op if no entries exceed the retention window
- **AND** the summary header from a prior compaction is preserved in the output
