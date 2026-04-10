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
synrepo SHALL label card fields by source store and freshness so agents can distinguish current structural facts from optional commentary.

#### Scenario: Attach commentary to a card
- **WHEN** a card includes both structural data and optional commentary
- **THEN** graph-backed fields remain identifiable as canonical
- **AND** overlay-backed fields are labeled with freshness state rather than presented as equivalent truth
