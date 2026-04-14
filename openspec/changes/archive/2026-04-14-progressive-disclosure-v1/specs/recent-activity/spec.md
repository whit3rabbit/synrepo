## ADDED Requirements

### Requirement: Expose a bounded recent-activity surface
synrepo SHALL expose a `synrepo_recent_activity` MCP tool that returns a bounded lane of synrepo's own operational events. The tool SHALL accept: `kinds?` (array filter: `reconcile | repair | cross_link | overlay_refresh | hotspot`; default all), `limit?` (integer, default 20, maximum 200), and `since?` (RFC 3339 timestamp). At least one of `limit` or `since` SHALL bound the response; the tool SHALL apply the default limit when neither is specified. The tool SHALL NOT record or surface agent identity, prompt content, or agent-interaction history.

#### Scenario: Agent requests recent reconcile outcomes
- **WHEN** an agent invokes `synrepo_recent_activity` with `kinds: ["reconcile"]`
- **THEN** the tool returns the most recent persisted reconcile outcome with timestamp, file-count, symbol-count, triggering-events, and outcome string
- **AND** the entry is labeled `kind: "reconcile"` and `note: "single_entry"` because reconcile state persists only the last outcome

#### Scenario: Agent requests recent repair-log entries
- **WHEN** an agent invokes `synrepo_recent_activity` with `kinds: ["repair"]` and `limit: 10`
- **THEN** the tool returns at most 10 of the most recent `ResolutionLogEntry` rows from `repair-log.jsonl`
- **AND** each entry includes timestamp, repair surfaces in scope, actions taken, and outcome

#### Scenario: Agent requests recent cross-link events
- **WHEN** an agent invokes `synrepo_recent_activity` with `kinds: ["cross_link"]`
- **THEN** the tool returns the most recent rows from the `cross_link_audit` overlay table ordered by `event_at`
- **AND** each entry includes from-node, to-node, edge kind, event kind (generated/accepted/rejected), and timestamp

#### Scenario: Agent requests hotspot files
- **WHEN** an agent invokes `synrepo_recent_activity` with `kinds: ["hotspot"]`
- **THEN** the tool returns churn-hot files ranked by co-change count from the git-intelligence index
- **AND** when git history is unavailable the tool returns an empty list with `state: "unavailable"` rather than an error

#### Scenario: Tool refuses unbounded lookback
- **WHEN** `synrepo_recent_activity` is invoked without `limit` or `since`
- **THEN** the tool applies the default limit of 20 entries
- **AND** a request with neither `limit` nor `since` SHALL NOT return unbounded results

#### Scenario: Tool rejects over-limit requests
- **WHEN** `synrepo_recent_activity` is invoked with `limit` exceeding 200
- **THEN** the tool returns an explicit error rather than silently truncating
- **AND** no event data is returned

### Requirement: Expose recent-activity data via synrepo status --recent flag
synrepo SHALL expose a `--recent` flag on `synrepo status` that prints a bounded summary of the same operational events returned by `synrepo_recent_activity`, using the default limit and all event kinds.

#### Scenario: CLI user requests recent activity
- **WHEN** a user runs `synrepo status --recent`
- **THEN** the output includes the most recent reconcile outcome, up to 5 recent repair-log entries, and any cross-link events from the last 24 hours
- **AND** the output respects the default limit of 20 total entries

#### Scenario: Machine-readable recent activity
- **WHEN** a user runs `synrepo status --recent --json`
- **THEN** the output is a JSON object with the same bounded event list returned by `synrepo_recent_activity`
