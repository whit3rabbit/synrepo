## ADDED Requirements

### Requirement: Define FileCard git intelligence surfacing

`FileCard` SHALL carry a `git_intelligence` field that exposes git-derived recency, hotspot touches, ownership hints, and co-change partners for the file. The field SHALL be absent at `tiny` budget. At `normal` and `deep` budget, the field SHALL be populated when a git context can be established for the repository; the payload SHALL carry a readiness status that distinguishes `ready` from degraded states. When git context cannot be established at all, the field SHALL be `null` rather than a synthetic degraded payload.

#### Scenario: Populate git intelligence at normal budget
- **WHEN** a `FileCard` is requested at `normal` budget and repository history is available
- **THEN** `git_intelligence` carries the readiness status, recent commits, hotspot touches, ownership hint, and co-change partners for the file
- **AND** the payload is labeled as `git_observed` rather than presented as canonical code truth

#### Scenario: Absent at tiny budget
- **WHEN** a `FileCard` is requested at `tiny` budget
- **THEN** `git_intelligence` is absent from the response

#### Scenario: Degraded history with readiness signal
- **WHEN** a `FileCard` is requested at `normal` or `deep` budget and history is degraded or the file has no sampled touches
- **THEN** `git_intelligence` is populated with a non-`ready` readiness status and empty sub-fields rather than silently elided
- **AND** downstream consumers can branch on the readiness status instead of inferring from empty commits

#### Scenario: Git context unavailable
- **WHEN** a `FileCard` is requested and no git context can be opened for the repository
- **THEN** `git_intelligence` is `null`
- **AND** the absence is not reported as a degraded readiness state on a partial payload

### Requirement: Define SymbolCard last-change with explicit granularity

`SymbolCard.last_change` SHALL carry a structured last-change summary or be `null`. When populated, the payload SHALL include a revision identifier, author name, committed-at timestamp, and a `granularity` label drawn from `file`, `symbol`, or `unknown`. The `granularity` label SHALL accurately reflect the precision of the underlying data source; implementations SHALL NOT label a file-level approximation as `symbol`. At `tiny` budget the field SHALL be absent. At `normal` budget the field SHALL be populated when history is available, without the summary. At `deep` budget the field SHALL additionally include the folded one-line commit summary when available.

#### Scenario: Populate last_change at normal budget with file granularity
- **WHEN** a `SymbolCard` is requested at `normal` budget and history for its containing file is available
- **THEN** `last_change` carries revision, author name, committed-at timestamp, and `granularity: "file"`
- **AND** the folded commit summary is omitted

#### Scenario: Populate last_change at deep budget with summary
- **WHEN** a `SymbolCard` is requested at `deep` budget and history for its containing file is available
- **THEN** `last_change` carries revision, author name, committed-at timestamp, `granularity: "file"`, and the folded one-line summary

#### Scenario: Absent at tiny budget
- **WHEN** a `SymbolCard` is requested at `tiny` budget
- **THEN** `last_change` is absent from the response

#### Scenario: Unknown granularity when history is degraded
- **WHEN** a `SymbolCard` is requested at `normal` or `deep` budget and git history is degraded or the containing file has no sampled touches
- **THEN** `last_change` is either `null` or carries `granularity: "unknown"` with the readiness reason discoverable from the accompanying `FileCard.git_intelligence.status` when both cards are read together
- **AND** the card does not invent a revision or author

#### Scenario: Upgrade path to symbol granularity is non-breaking
- **WHEN** a future change wires symbol-level body-hash tracking
- **THEN** the `granularity` value may transition from `"file"` to `"symbol"` without otherwise altering the `last_change` shape
- **AND** consumers that read `revision`, `author_name`, and `committed_at_unix` continue to function without modification
