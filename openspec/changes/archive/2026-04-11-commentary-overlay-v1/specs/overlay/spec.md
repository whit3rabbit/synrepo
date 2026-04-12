## MODIFIED Requirements

### Requirement: Define overlay-only machine-authored content
synrepo SHALL define machine-authored commentary as overlay content that annotates graph nodes without overriding graph-backed truth. Commentary covers synthesized explanations, usage summaries, and contextual annotations. Evidence-verified proposed cross-links are out of scope for this spec; see `openspec/specs/overlay-links/spec.md`.

#### Scenario: Record a synthesized explanation
- **WHEN** the synthesis pipeline creates commentary for a card
- **THEN** the resulting content is stored and surfaced as overlay data
- **AND** the product does not present it as canonical graph truth

#### Scenario: Reject commentary that would shadow a graph fact
- **WHEN** a commentary entry references a field that is already authoritative in the graph
- **THEN** the overlay store records the commentary separately
- **AND** the surfaced output labels the commentary source distinctly from the graph-backed value

### Requirement: Define minimum overlay provenance fields
synrepo SHALL define the minimum provenance required for persisted commentary entries: source revision at generation time, producing pass identifier, model identity, generation timestamp, and evidence references when applicable. Missing provenance degrades the entry to an invalid state and prevents it from being surfaced to users in normal operation.

#### Scenario: Persist a commentary entry with full provenance
- **WHEN** the overlay stores a commentary entry
- **THEN** the stored artifact carries: source revision, producing pass, model identity, generation timestamp, and any evidence spans cited
- **AND** missing provenance invalidates the artifact for normal user-facing use

#### Scenario: Surface a commentary entry with missing provenance
- **WHEN** a commentary entry is present but lacks one or more required provenance fields
- **THEN** the overlay contract classifies it as invalid
- **AND** the entry is withheld from normal user responses and flagged in audit queries

### Requirement: Define commentary freshness states
synrepo SHALL define five freshness states for commentary entries: `fresh` (provenance revision matches current graph), `stale` (provenance revision predates the current graph revision for the annotated node), `invalid` (entry is present but missing required provenance), `missing` (no entry exists for a requested node), and `unsupported` (the runtime does not yet support commentary for this node kind). Each state has a defined surfacing behavior.

#### Scenario: Return stale commentary
- **WHEN** an agent requests a card with commentary whose provenance revision does not match the current graph revision for the annotated node
- **THEN** the overlay contract classifies the entry as stale
- **AND** the entry is surfaced with an explicit staleness label so the caller can decide whether to rely on it or request a refresh

#### Scenario: Surface commentary for a node with no stored entry
- **WHEN** an agent requests a card with commentary for a node that has no overlay entry
- **THEN** the overlay contract classifies the state as missing
- **AND** the response labels the absence explicitly rather than returning an empty or silent field

#### Scenario: Surface commentary for an unsupported node kind
- **WHEN** an agent requests commentary for a node kind that the commentary pipeline does not yet support
- **THEN** the overlay contract classifies the state as unsupported
- **AND** the response labels the state honestly rather than generating a placeholder

### Requirement: Define commentary retrieval boundaries
synrepo SHALL define commentary as optional, explicitly labeled, and never silently merged into graph-backed fact fields. Commentary fields are surfaced alongside structural output under a distinct source-store label. Budget controls determine whether commentary is included in a given response; budget-withheld commentary is equivalent to the missing state for the purposes of that response.

#### Scenario: Return graph output without commentary at tight budget
- **WHEN** a card request fits within the structural budget tier and commentary inclusion would exceed the budget
- **THEN** the response returns only graph-backed content
- **AND** the commentary source-store label is omitted or marked budget-withheld, not silently absent

#### Scenario: Return commentary alongside graph output at expanded budget
- **WHEN** a card request allows commentary inclusion and the entry is fresh
- **THEN** commentary is appended under its source-store label after structural content
- **AND** the structural content is not modified or re-ordered to accommodate the commentary

### Requirement: Define overlay audit trail exposure
synrepo SHALL define which overlay provenance details are user-visible, which remain internal, and how missing or conflicting audit data affects surfaced content. User-visible provenance includes: freshness state, source revision reference, and producing pass identifier. Model identity and internal evidence spans are available on explicit audit queries but not included in default responses.

#### Scenario: Inspect an overlay-backed response
- **WHEN** a user or agent asks for provenance behind commentary content
- **THEN** synrepo exposes the user-visible provenance fields: freshness state, source revision, and producing pass
- **AND** conflicting or incomplete audit data produces a visible degraded state, not silent omission

#### Scenario: Query model identity for audit purposes
- **WHEN** an operator runs an audit query against an overlay entry
- **THEN** synrepo can return the stored model identity and evidence references
- **AND** this data is not included in default card responses

### Requirement: Define commentary cost and generation controls
synrepo SHALL define budget limits, lazy versus eager generation policy, and refresh behavior for commentary generation. Commentary is generated lazily on first request by default. Eager pre-generation is opt-in and bounded by a configurable cost limit. Refresh follows the same lazy default unless the caller explicitly requests a forced refresh for a stale or invalid entry.

#### Scenario: Generate commentary on first request
- **WHEN** a card request includes commentary and no entry exists for the node
- **THEN** the overlay pipeline generates commentary lazily at request time if within budget
- **AND** the generated entry is persisted with full provenance before being returned

#### Scenario: Withhold commentary when budget is exceeded
- **WHEN** a commentary generation or inclusion request would exceed the configured cost limit
- **THEN** the overlay contract withholds the commentary and labels the response accordingly
- **AND** no partial or truncated commentary is synthesized or surfaced

## REMOVED Requirements

### Requirement: Define bounded evidence-verified linking
**Reason**: Cross-link candidate generation, verification, confidence scoring, and review surfaces are a distinct capability from commentary and belong in their own durable spec. Keeping this requirement here created scope ambiguity for the Milestone 5 commentary implementation slice.
**Migration**: This requirement moves to `openspec/specs/overlay-links/spec.md` as "Define evidence-verified cross-link candidates." No runtime behavior is removed; the feature was not yet implemented.
