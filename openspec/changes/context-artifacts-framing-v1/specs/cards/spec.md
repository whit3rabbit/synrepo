## ADDED Requirements

### Requirement: Treat cards as artifact delivery packets
synrepo SHALL treat cards as the current native compact delivery packet for code artifacts and task contexts. Existing card contracts, budget tiers, source labels, and context accounting SHALL remain valid when docs refer to cards as delivery packets.

#### Scenario: Agent requests a card
- **WHEN** an agent requests an existing card type
- **THEN** the response preserves the existing card schema and budget behavior
- **AND** docs may describe that response as a delivered code artifact or task context packet

#### Scenario: Contributor updates card docs
- **WHEN** a contributor updates card documentation
- **THEN** they preserve the distinction between graph-backed fields and advisory overlay-backed fields
- **AND** they do not introduce a new card schema solely for the framing change
