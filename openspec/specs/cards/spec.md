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

