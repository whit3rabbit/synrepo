## MODIFIED Requirements

### Requirement: Surface file-scoped git intelligence on cards

synrepo SHALL surface per-file git intelligence (sampled commit history, hotspot touches, ownership hints, co-change partners) through the `FileCard.git_intelligence` field and SHALL project the most recent first-parent touch of a file into the `last_change` field of `SymbolCard`s defined in that file when symbol-scoped revision data is not available. When a symbol has a stored `last_modified_rev` derived from `body_hash` transitions in the sampled history, the `last_change` payload SHALL use that symbol-scoped revision and label it with `granularity: "symbol"`. The surfacing layer SHALL obey the repository's configured `git_commit_depth` as the sampling budget and SHALL treat surfaced git intelligence as `git_observed` authority, never as canonical descriptive truth over parser-observed code facts.

#### Scenario: Card surfaces hotspot and ownership from sampled history
- **WHEN** an agent requests a `FileCard` for a file that appears in the sampled first-parent history
- **THEN** the card carries the hotspot touch count, ownership hint, and co-change partners derived from the sampling window
- **AND** the payload is labeled so the agent can distinguish it from parser-observed structural edges

#### Scenario: Symbol last_change uses symbol-scoped revision when available
- **WHEN** an agent requests a `SymbolCard` and the symbol has a stored `last_modified_rev` from body-hash diffing
- **THEN** the card's `last_change` carries the revision, author, and timestamp from that symbol-scoped commit, labeled `granularity: "symbol"`
- **AND** the commit summary is included at `deep` budget per the cards spec

#### Scenario: Symbol last_change falls back to file granularity
- **WHEN** an agent requests a `SymbolCard` and the symbol has no stored `last_modified_rev` (new symbol, no body-hash transition in the sampling window, or degraded history)
- **THEN** the card's `last_change` carries the containing file's most recent first-parent commit, labeled `granularity: "file"`
- **AND** the behavior is identical to the pre-upgrade projection

#### Scenario: Surfacing respects the configured sampling budget
- **WHEN** per-file git intelligence is surfaced on a card
- **THEN** the sampling honors `git_commit_depth` from the runtime config
- **AND** no additional history walks happen outside that budget

## ADDED Requirements

### Requirement: Derive symbol-scoped revisions from body-hash transitions

synrepo SHALL derive `first_seen_rev` and `last_modified_rev` for each `SymbolNode` by parsing the file at each sampled commit within the configured `git_commit_depth` and comparing `body_hash` values for symbols whose qualified names match across adjacent commits. The derivation SHALL occur during stage 5 (git mining) and SHALL store the resulting revisions on the `SymbolNode` row as `git_observed` authority.

#### Scenario: Body-hash transition found in sampled window
- **WHEN** a symbol's `body_hash` at the current HEAD differs from its `body_hash` at a sampled historical commit
- **THEN** `last_modified_rev` is set to the first (newest) commit where the hash differs
- **AND** `first_seen_rev` is set to the oldest sampled commit where the symbol's qualified name appears

#### Scenario: No body-hash transition in sampled window
- **WHEN** a symbol's `body_hash` is identical across all sampled commits where its qualified name appears
- **THEN** `last_modified_rev` is `NULL` and the card compiler falls back to file-level granularity
- **AND** `first_seen_rev` is still set to the oldest sampled commit where the name appears

#### Scenario: New symbol not present in historical commits
- **WHEN** a symbol's qualified name does not appear in any sampled historical parse of its file
- **THEN** both `first_seen_rev` and `last_modified_rev` are `NULL`
- **AND** the card compiler falls back to file-level granularity

#### Scenario: Degraded history skips symbol-scoped derivation
- **WHEN** git history is degraded (shallow clone, detached HEAD) or the file has no sampled commits
- **THEN** `first_seen_rev` and `last_modified_rev` remain `NULL` for all symbols in that file
- **AND** the card compiler reports `granularity: "file"` or `granularity: "unknown"` per existing degraded-history rules
