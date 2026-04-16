## Purpose
Define the card contracts, budget tiers, and source-labeling rules that make cards the primary product surface for agents.
## Requirements
### Requirement: Define card types as the product surface
synrepo SHALL define card contracts for the core structural card types that agents use to orient, route edits, assess impact, and inspect test coverage.

#### Scenario: Ask for context about a symbol
- **WHEN** an agent requests a symbol-focused context packet
- **THEN** the cards spec defines the required structural fields for the returned card type
- **AND** the response can be understood without reading arbitrary source files first

### Requirement: Define budget tiers and truncation priority
synrepo SHALL define explicit card budget tiers and the order in which lower-priority card fields are truncated when a response must fit a token budget. Budget tiers SHALL be documented as a three-surface progressive-disclosure protocol — `tiny` for orientation, `normal` for local understanding, `deep` for fetch-on-demand — not as an internal truncation knob, so agents escalate intentionally rather than defaulting to the largest tier.

#### Scenario: Return a tiny card
- **WHEN** a tool is asked for a `tiny` budget response
- **THEN** the card contract defines the minimal required fields
- **AND** truncation happens by declared priority instead of accidental omission

### Requirement: Budget tiers implement a progressive-disclosure protocol
synrepo's three budget tiers (`tiny`, `normal`, `deep`) SHALL be treated as a deliberate three-surface interaction pattern, not merely a size knob. Agents SHALL begin with `tiny` or `normal` to orient, and escalate to `deep` only when a specific field requires it. The card compiler SHALL maintain this contract across all card types: `tiny` returns index-quality signals, `normal` returns neighborhood-quality context (signature, neighbors, co-change partners), and `deep` returns inspection-quality detail (source body, full overlay content, full neighbor cards).

#### Scenario: Agent orients with tiny budget first
- **WHEN** an agent begins work on an unfamiliar symbol or file
- **THEN** a `tiny` budget request returns enough signal (name, kind, location, edge counts) to decide whether to escalate
- **AND** no source body, overlay commentary, or neighbor detail is included at `tiny` budget

#### Scenario: Agent escalates from tiny to normal
- **WHEN** an agent determines a symbol is relevant after a `tiny`-budget response
- **THEN** a `normal` budget request adds signature, doc comment, co-change partners, and structural neighbor summaries
- **AND** source body and full overlay content remain absent at `normal` budget

#### Scenario: Agent escalates to deep only for inspection
- **WHEN** an agent needs to read or edit a symbol's implementation
- **THEN** a `deep` budget request adds source body, full overlay commentary, full neighbor cards, and proposed cross-links
- **AND** the escalation is explicit — callers do not receive `deep` content unless they request it

#### Scenario: Budget tier is preserved in the response
- **WHEN** any card is returned by the card compiler or an MCP tool
- **THEN** the response includes a field identifying the budget tier used
- **AND** callers can distinguish a `tiny`-budget response from a `normal` or `deep` response without inspecting field presence

### Requirement: Distinguish graph-backed and overlay-backed card fields
synrepo SHALL label card fields by source store and freshness so agents can distinguish current structural facts from optional overlay content. The `overlay_commentary` field on `SymbolCard` SHALL carry one of five freshness states: `fresh`, `stale`, `invalid`, `missing`, or `unsupported`. The `proposed_links` field on `SymbolCard` and `FileCard` SHALL carry zero or more surfaced cross-link candidates, each labeled with its overlay source store, freshness state (`fresh` | `stale` | `source_deleted` | `invalid` | `missing`), and confidence tier (`high` | `review_queue`). `below_threshold` candidates SHALL NOT appear in `proposed_links`. At `tiny` and `normal` budget tiers, both `overlay_commentary` and `proposed_links` are omitted and the response MAY include `commentary_state: "budget_withheld"` and `links_state: "budget_withheld"` so callers can distinguish budget-withheld from absent. At `deep` budget, each field is populated if content exists; otherwise the state label reflects the actual absence reason.

#### Scenario: Attach commentary to a card
- **WHEN** a card includes both structural data and optional commentary
- **THEN** graph-backed fields remain identifiable as canonical
- **AND** overlay-backed fields are labeled with freshness state rather than presented as equivalent truth

#### Scenario: Return commentary state at tight budget
- **WHEN** a `SymbolCard` is requested at `tiny` or `normal` budget
- **THEN** `overlay_commentary` is omitted from the response
- **AND** the response includes `commentary_state: "budget_withheld"` so callers can distinguish this from an absent entry

#### Scenario: Return fresh commentary at deep budget
- **WHEN** a `SymbolCard` is requested at `deep` budget and a fresh commentary entry exists
- **THEN** `overlay_commentary` is populated with the commentary text and `freshness: "fresh"`
- **AND** the structural fields are not modified or reordered to accommodate the commentary

#### Scenario: Return stale commentary at deep budget
- **WHEN** a `SymbolCard` is requested at `deep` budget and a stale commentary entry exists
- **THEN** `overlay_commentary` is populated with the commentary text and `freshness: "stale"`
- **AND** the staleness label is surfaced to callers rather than withheld

#### Scenario: Return missing state when no commentary exists
- **WHEN** a `SymbolCard` is requested at `deep` budget and no commentary entry exists for the node
- **THEN** `overlay_commentary` is `null` and `commentary_state` is `"missing"`
- **AND** the absence is labeled explicitly; no empty or placeholder commentary is generated

#### Scenario: Return unsupported state for node kinds without commentary
- **WHEN** a `SymbolCard` is requested at `deep` budget for a node kind that the commentary pipeline does not support
- **THEN** `overlay_commentary` is `null` and `commentary_state` is `"unsupported"`

#### Scenario: Return proposed links at deep budget
- **WHEN** a `SymbolCard` or `FileCard` is requested at `deep` budget and one or more cross-link candidates involving the node exist at `high` or `review_queue` tier
- **THEN** `proposed_links` is populated with the candidate entries, each carrying its endpoint IDs, overlay edge kind, confidence tier, freshness state, and cited-span count
- **AND** the structural edge fields on the card remain untouched by the overlay content
- **AND** `below_threshold` candidates are excluded from the response

#### Scenario: Return proposed links budget-withheld at tight budgets
- **WHEN** a card is requested at `tiny` or `normal` budget
- **THEN** `proposed_links` is omitted
- **AND** the response includes `links_state: "budget_withheld"` so callers can distinguish from absent

#### Scenario: Return missing state when no proposed links exist
- **WHEN** a card is requested at `deep` budget and no cross-link candidates at `high` or `review_queue` tier exist for the node
- **THEN** `proposed_links` is an empty list and `links_state` is `"missing"`

#### Scenario: Stale candidate surfaces with explicit staleness label
- **WHEN** a card is requested at `deep` budget and a cross-link candidate's stored endpoint hash no longer matches the current graph
- **THEN** the candidate appears in `proposed_links` with `freshness: "stale"`
- **AND** the stale label is surfaced to callers rather than withheld

### Requirement: Define DecisionCard as an optional rationale output
synrepo SHALL define DecisionCard as an optional card type returned when a queried node has incoming `Governs` edges from ConceptNodes with rationale content. DecisionCard is backed exclusively by `HumanDeclared` or `ParserObserved` ConceptNodes; overlay content SHALL NOT populate DecisionCard fields. The card SHALL distinguish rationale from current code behavior by labeling its source as human-authored.

#### Scenario: Return a DecisionCard when rationale exists
- **WHEN** an agent queries a node that has incoming Governs edges from one or more ConceptNodes
- **THEN** the response MAY include a DecisionCard containing the decision title, status (if available), decision text, and the IDs of governed nodes
- **AND** the DecisionCard source is labeled as human-authored, not as structural observation

#### Scenario: No DecisionCard when no rationale is linked
- **WHEN** an agent queries a node with no incoming Governs edges
- **THEN** no DecisionCard is included in the response
- **AND** the structural card is returned unchanged

#### Scenario: DecisionCard does not override structural truth
- **WHEN** a DecisionCard describes a design decision that conflicts with observed code behavior
- **THEN** the structural card fields reflect current observed code state
- **AND** the DecisionCard content is labeled as rationale, not as a code fact
- **AND** no structural field is modified to match the DecisionCard content

### Requirement: Define DecisionCard budget tier behavior
synrepo SHALL apply the same `tiny` / `normal` / `deep` budget tier model to DecisionCard as to other card types. At `tiny` tier, DecisionCard includes only the decision title and governed node IDs. At `normal` tier, it adds status and a truncated decision body. At `deep` tier, it includes the complete decision body and all linked ConceptNode IDs.

#### Scenario: Return a tiny DecisionCard
- **WHEN** a tool requests a `tiny` budget response for a node with linked rationale
- **THEN** the DecisionCard includes only title and governed node IDs
- **AND** the decision body is omitted

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


### Requirement: Define PublicAPICard

A `PublicAPICard` SHALL aggregate the exported API surface of a directory: public symbols, public entry points, and (at `deep` budget) recently changed public API. Visibility is inferred from `SymbolNode.signature`: a symbol is public if its signature starts with `pub`. This heuristic is Rust-specific; non-Rust directories return empty symbol lists in v1.

**Budget-tier field gating:**
- `tiny`: `path`, `public_symbol_count`, `approx_tokens`, `source_store` only; symbol lists are empty
- `normal`: additionally populates `public_symbols` and `public_entry_points`; `PublicAPIEntry.last_change` present without commit summary
- `deep`: additionally populates `recent_api_changes` (30-day window); `last_change` includes commit summary

#### Scenario: Tiny budget returns count only
- **WHEN** a `PublicAPICard` is requested at `tiny` budget for a directory containing Rust files with public symbols
- **THEN** `public_symbol_count` is populated and greater than zero
- **AND** `public_symbols` and `public_entry_points` are empty
- **AND** `recent_api_changes` is absent or empty

#### Scenario: Normal budget materialises public symbol list
- **WHEN** a `PublicAPICard` is requested at `normal` budget
- **THEN** `public_symbols` is populated with one entry per public symbol from direct-child files
- **AND** each entry carries `id`, `name`, `kind`, `signature`, and `location`
- **AND** private symbols (no `pub` prefix on signature) are excluded

#### Scenario: Deep budget includes recent API changes
- **WHEN** a `PublicAPICard` is requested at `deep` budget and git context is available
- **THEN** `recent_api_changes` contains public symbols whose containing file was last touched within 30 days
- **AND** each entry carries `last_change` with commit summary

#### Scenario: Public entry points are a subset of public symbols
- **WHEN** a `PublicAPICard` is requested at `normal` or `deep` budget
- **THEN** every entry in `public_entry_points` also appears in `public_symbols`
- **AND** only symbols matching an entry-point detection rule appear in `public_entry_points`

#### Scenario: Private symbols are excluded at all budgets
- **WHEN** a `PublicAPICard` is requested for a directory containing symbols without a `pub` signature prefix
- **THEN** those symbols do not appear in `public_symbols`, `public_entry_points`, or `recent_api_changes` at any budget tier

#### Scenario: Non-Rust directory returns empty public_symbols
- **WHEN** a `PublicAPICard` is requested for a directory containing only Python, TypeScript, or Go files
- **THEN** `public_symbols` is empty and `public_symbol_count` is zero (v1 limitation; visibility detection is Rust-specific)

### Requirement: Define CallPathCard

synrepo SHALL define `CallPathCard` as a graph-derived card that traces execution paths from entry points to a target symbol using backward BFS over `Calls` edges. All path data SHALL be sourced exclusively from the graph (`source_store: "graph"`). No LLM involvement and no overlay content SHALL appear in a `CallPathCard`. When no path is found, the card SHALL return an empty path list rather than a spurious result.

#### Scenario: Compile a CallPathCard for a reachable symbol
- **WHEN** `call_path_card(target, budget)` is called on a symbol that has at least one `Calls` edge chain leading to an entry point
- **THEN** the returned card includes at least one `CallPath` entry
- **AND** each `CallPath` lists `CallPathEdge` records from the entry point symbol to the target
- **AND** `source_store` is `"graph"`

#### Scenario: Compile a CallPathCard for an unreachable symbol
- **WHEN** `call_path_card(target, budget)` is called on a symbol with no `Calls` edges leading to any entry point within the depth budget
- **THEN** the returned card has an empty `paths` list
- **AND** `paths_omitted` is 0

#### Scenario: Path truncation due to depth limit
- **WHEN** the shortest path from an entry point to the target exceeds the depth budget (8 hops at normal/tiny, 12 at deep)
- **THEN** the card includes a truncated path with `truncated: true` on the final `CallPathEdge`

#### Scenario: Path deduplication
- **WHEN** there are more than 3 distinct paths from an entry point to a target
- **THEN** the card includes at most 3 paths for that (entry, target) pair
- **AND** the count of omitted paths is recorded in `paths_omitted`

### Requirement: Define TestSurfaceCard

synrepo SHALL define `TestSurfaceCard` as a graph-derived card that discovers test functions related to a given scope (file path or directory). All test data SHALL be sourced exclusively from the graph (`source_store: "graph"`). No LLM involvement and no overlay content SHALL appear in a `TestSurfaceCard`. When no tests are found, the card SHALL return an empty `tests` list rather than a spurious result.

#### Scenario: Compile a TestSurfaceCard for a file with associated tests
- **WHEN** `test_surface_card(scope, budget)` is called on a file that has test files associated by path convention
- **THEN** the returned card includes at least one `TestEntry` record
- **AND** each `TestEntry` includes the test symbol's node ID, qualified name, and containing file path
- **AND** `source_store` is `"graph"`

#### Scenario: Compile a TestSurfaceCard when no tests exist
- **WHEN** `test_surface_card(scope, budget)` is called and no test files match any association rule for the given scope
- **THEN** the returned card has an empty `tests` list
- **AND** no error is raised

#### Scenario: Test discovery via path convention
- **WHEN** a source file has test files matching sibling patterns (`*_test.rs`, `test_*.py`, `*.test.ts`, `*.spec.ts`), parallel test directory (`tests/<stem>`), or nested test module (`tests/`, `__tests__/`)
- **THEN** test symbols from those files are included as `TestEntry` records
- **AND** `association` is set to `"path_convention"`, `"symbol_kind"`, or `"both"` based on which signals matched

#### Scenario: Tiny budget returns counts only
- **WHEN** a `TestSurfaceCard` is requested at `tiny` budget
- **THEN** the card includes only `test_file_count` and `test_symbol_count`
- **AND** individual `TestEntry` records are omitted
