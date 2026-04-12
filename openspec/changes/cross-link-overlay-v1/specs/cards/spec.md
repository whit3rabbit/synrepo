## MODIFIED Requirements

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
