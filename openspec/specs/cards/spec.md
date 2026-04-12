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
synrepo SHALL define explicit card budget tiers and the order in which lower-priority card fields are truncated when a response must fit a token budget.

#### Scenario: Return a tiny card
- **WHEN** a tool is asked for a `tiny` budget response
- **THEN** the card contract defines the minimal required fields
- **AND** truncation happens by declared priority instead of accidental omission

### Requirement: Distinguish graph-backed and overlay-backed card fields
synrepo SHALL label card fields by source store and freshness so agents can distinguish current structural facts from optional commentary. The `overlay_commentary` field on `SymbolCard` SHALL carry one of five freshness states: `fresh` (commentary matches current source), `stale` (source has changed since generation), `invalid` (entry is present but missing required provenance), `missing` (no entry exists), or `unsupported` (commentary is not defined for this node kind). At `tiny` and `normal` budget tiers, the `overlay_commentary` field is omitted and the response MAY include a `commentary_state: "budget_withheld"` label so callers can distinguish budget-withheld from absent. At `deep` budget, the field is populated if an entry exists; otherwise the state label reflects the actual absence reason.

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
