## Purpose
Define how synrepo uses repository history as non-canonical input for routing, impact analysis, and change-risk enrichment.

## Requirements

### Requirement: Define git-derived facts as secondary authority
synrepo SHALL treat ownership, hotspots, co-change, last meaningful change, churn, and related history signals as `git_observed` facts that enrich routing without becoming canonical descriptive truth over parser-observed code facts.

#### Scenario: Use history to rank edit targets
- **WHEN** a user asks where to edit or what might break
- **THEN** git-derived signals may influence ranking and explanation
- **AND** they do not override parser-observed structure or human-declared rationale

### Requirement: Define git-intelligence outputs
synrepo SHALL define which card fields and MCP responses may expose git-derived enrichments, including ownership hints, hotspot signals, co-change partners, and last meaningful change summaries.

#### Scenario: Read a change-risk card
- **WHEN** a card includes git-derived enrichment
- **THEN** the contract identifies the history-backed fields and their authority
- **AND** the user can distinguish them from graph structure and overlay commentary

### Requirement: Define degraded history behavior
synrepo SHALL define how git intelligence behaves on shallow clones, detached HEADs, missing history, ignored submodules, and other degraded repository states.

#### Scenario: Run on a shallow clone
- **WHEN** the repository does not provide enough history for normal git mining
- **THEN** synrepo reports degraded git intelligence rather than inventing stable-seeming results
- **AND** structural graph behavior continues without requiring full history

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

### Requirement: Map git intelligence availability to readiness
Git intelligence SHALL report readiness states that distinguish ready history, degraded history, unavailable git context, and intentionally absent git usage.

#### Scenario: Repository has no git context
- **WHEN** synrepo cannot open git context for the repository
- **THEN** the readiness matrix marks git intelligence as unavailable
- **AND** graph-backed parser facts remain usable without git-derived ownership or co-change claims

#### Scenario: History is shallow or degraded
- **WHEN** git context exists but history is shallow, detached, or missing sampled touches
- **THEN** the readiness matrix marks git intelligence as degraded
- **AND** cards that depend on git labels expose the same degraded state rather than inventing ownership or co-change facts
