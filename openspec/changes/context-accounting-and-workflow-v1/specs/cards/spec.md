## ADDED Requirements

### Requirement: Honor numeric context caps for card sets
Card-set responses SHALL accept an optional numeric token cap in addition to the existing budget tier.

#### Scenario: Numeric cap is lower than full result
- **WHEN** a card-set response would exceed the numeric token cap
- **THEN** synrepo drops lower-ranked optional cards or fields before returning the response
- **AND** `context_accounting.truncation_applied` is true

### Requirement: Preserve existing budget tiers
The `tiny`, `normal`, and `deep` budget tiers SHALL remain valid and backward compatible.

#### Scenario: Caller omits numeric cap
- **WHEN** a caller requests a card with only a budget tier
- **THEN** synrepo applies the existing tier behavior
- **AND** the response still includes context accounting metadata
