## MODIFIED Requirements

### Requirement: Define SymbolCard last-change with explicit granularity

`SymbolCard.last_change` SHALL carry a structured last-change summary or be `null`. When populated, the payload SHALL include a revision identifier, author name, committed-at timestamp, and a `granularity` label drawn from `file`, `symbol`, or `unknown`. The `granularity` label SHALL accurately reflect the precision of the underlying data source. When symbol-scoped revision data is available (the symbol has a stored `last_modified_rev` from body-hash diffing), the payload SHALL use `granularity: "symbol"` and reference the commit that last modified the symbol's body. When only file-level data is available, the payload SHALL use `granularity: "file"` and reference the most recent commit touching the containing file. At `tiny` budget the field SHALL be absent. At `normal` budget the field SHALL be populated when history is available, without the summary. At `deep` budget the field SHALL additionally include the folded one-line commit summary when available.

#### Scenario: Populate last_change at normal budget with symbol granularity
- **WHEN** a `SymbolCard` is requested at `normal` budget and the symbol has a stored `last_modified_rev`
- **THEN** `last_change` carries revision, author name, committed-at timestamp, and `granularity: "symbol"`
- **AND** the folded commit summary is omitted

#### Scenario: Populate last_change at deep budget with symbol granularity and summary
- **WHEN** a `SymbolCard` is requested at `deep` budget and the symbol has a stored `last_modified_rev`
- **THEN** `last_change` carries revision, author name, committed-at timestamp, `granularity: "symbol"`, and the folded one-line summary

#### Scenario: Populate last_change at normal budget with file granularity fallback
- **WHEN** a `SymbolCard` is requested at `normal` budget and the symbol has no stored `last_modified_rev` but the containing file has sampled history
- **THEN** `last_change` carries revision, author name, committed-at timestamp, and `granularity: "file"`
- **AND** the folded commit summary is omitted

#### Scenario: Populate last_change at deep budget with file granularity and summary
- **WHEN** a `SymbolCard` is requested at `deep` budget and the symbol has no stored `last_modified_rev` but the containing file has sampled history
- **THEN** `last_change` carries revision, author name, committed-at timestamp, `granularity: "file"`, and the folded one-line summary

#### Scenario: Absent at tiny budget
- **WHEN** a `SymbolCard` is requested at `tiny` budget
- **THEN** `last_change` is absent from the response

#### Scenario: Unknown granularity when history is degraded
- **WHEN** a `SymbolCard` is requested at `normal` or `deep` budget and git history is degraded or the containing file has no sampled touches
- **THEN** `last_change` is either `null` or carries `granularity: "unknown"` with the readiness reason discoverable from the accompanying `FileCard.git_intelligence.status` when both cards are read together
- **AND** the card does not invent a revision or author
