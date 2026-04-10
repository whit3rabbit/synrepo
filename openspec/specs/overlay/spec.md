## Purpose
Define the machine-authored overlay for commentary, proposed links, freshness, and review surfaces without allowing it to override graph truth.

## Requirements

### Requirement: Define overlay-only machine-authored content
synrepo SHALL define machine-authored commentary, proposed links, and findings as overlay content that never overrides graph-backed truth.

#### Scenario: Record a synthesized explanation
- **WHEN** the synthesis pipeline creates commentary for a card
- **THEN** the resulting content is stored and surfaced as overlay data
- **AND** the product does not present it as canonical graph truth

### Requirement: Define minimum overlay provenance fields
synrepo SHALL define the minimum provenance required for persisted overlay entries, including source revision, producing pass, model identity, and evidence metadata when applicable.

#### Scenario: Persist a proposed link
- **WHEN** the overlay stores a synthesized link or commentary entry
- **THEN** the stored artifact carries the minimum provenance needed to audit how it was generated
- **AND** missing provenance invalidates the artifact for normal user-facing use

### Requirement: Define overlay freshness and cost controls
synrepo SHALL define freshness states, staleness handling, and cost controls for commentary and proposed-link generation.

#### Scenario: Return stale commentary
- **WHEN** an agent requests a card with stale overlay commentary
- **THEN** the overlay contract defines how staleness is labeled and refreshed
- **AND** the system can preserve responsiveness without hiding freshness risk

### Requirement: Define bounded evidence-verified linking
synrepo SHALL define candidate generation, verification, confidence scoring, and review surfaces for proposed cross-links while keeping the result auditable and non-authoritative.

#### Scenario: Propose a code-to-prose relationship
- **WHEN** the system infers a possible cross-link with cited evidence
- **THEN** the overlay contract requires verification and confidence metadata
- **AND** the resulting link remains supplemental until backed by human declaration

### Requirement: Define overlay audit trail exposure
synrepo SHALL define which overlay provenance and audit details are user-visible, which remain internal, and how missing or conflicting audit data affects surfaced content.

#### Scenario: Inspect an overlay-backed response
- **WHEN** a user or agent asks for provenance behind overlay content
- **THEN** synrepo can expose the required audit details without implying canonical truth
- **AND** conflicting or incomplete audit data produces a visible degraded state
