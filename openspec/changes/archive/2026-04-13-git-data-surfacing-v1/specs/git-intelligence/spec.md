## ADDED Requirements

### Requirement: Surface file-scoped git intelligence on cards

synrepo SHALL surface per-file git intelligence (sampled commit history, hotspot touches, ownership hints, co-change partners) through the `FileCard.git_intelligence` field and SHALL project the most recent first-parent touch of a file into the `last_change` field of `SymbolCard`s defined in that file, labeled with `granularity: "file"`. The surfacing layer SHALL obey the repository's configured `git_commit_depth` as the sampling budget and SHALL treat surfaced git intelligence as `git_observed` authority, never as canonical descriptive truth over parser-observed code facts.

#### Scenario: Card surfaces hotspot and ownership from sampled history
- **WHEN** an agent requests a `FileCard` for a file that appears in the sampled first-parent history
- **THEN** the card carries the hotspot touch count, ownership hint, and co-change partners derived from the sampling window
- **AND** the payload is labeled so the agent can distinguish it from parser-observed structural edges

#### Scenario: Symbol last_change projects from containing file
- **WHEN** an agent requests a `SymbolCard` and the containing file has at least one sampled commit
- **THEN** the card's `last_change` carries the most recent first-parent commit's revision, author, and timestamp, labeled `granularity: "file"`
- **AND** the same upgrade contract applies: if symbol-level tracking lands later, only the `granularity` label changes

#### Scenario: Surfacing respects the configured sampling budget
- **WHEN** per-file git intelligence is surfaced on a card
- **THEN** the sampling honors `git_commit_depth` from the runtime config
- **AND** no additional history walks happen outside that budget

### Requirement: Report degraded git state explicitly on card payloads

When repository history is shallow, detached, missing, or otherwise degraded, synrepo SHALL report the degraded status on the card payload rather than returning empty fields that could be misread as "no churn". When no git context can be established for the repository at all, synrepo SHALL return `null` for `FileCard.git_intelligence` rather than a synthetic degraded payload, so that "git unavailable" and "git available but degraded" are distinguishable.

#### Scenario: Shallow clone produces degraded readiness on card
- **WHEN** a `FileCard` is compiled against a repository whose history is too shallow for normal git mining
- **THEN** `git_intelligence` is populated with a non-`ready` readiness status and empty sub-fields
- **AND** the card does not fabricate ownership, hotspot, or co-change values

#### Scenario: No git repository returns null git intelligence
- **WHEN** a `FileCard` is compiled against a directory that is not a git repository or where git context cannot be opened
- **THEN** `git_intelligence` is `null`
- **AND** `SymbolCard.last_change` is `null` for symbols defined in that file
