## MODIFIED Requirements

### Requirement: Require provenance and freshness in responses
synrepo SHALL define MCP response contracts that expose provenance, source-store labeling, and freshness behavior for all graph-backed and overlay-backed content. For overlay commentary specifically, the contract SHALL define the observable behavior for each of the four commentary states: present-and-fresh (commentary included with source label), present-and-stale (commentary included with explicit staleness label), absent (no commentary entry exists, label omitted or marked missing), and budget-withheld (commentary was not included due to budget; labeled as withheld, not as absent).

#### Scenario: Consume mixed-source context
- **WHEN** an MCP response contains both structural facts and optional overlay commentary
- **THEN** the caller can distinguish the source and freshness of each returned field
- **AND** the contract prevents silent trust escalation

#### Scenario: Receive a response with stale commentary
- **WHEN** an MCP response includes overlay commentary whose provenance revision predates the current graph revision
- **THEN** the response labels the commentary field with its staleness state
- **AND** the structural content is presented without modification regardless of commentary freshness

#### Scenario: Receive a response where commentary was budget-withheld
- **WHEN** a card request does not include commentary because including it would exceed the budget tier
- **THEN** the response marks the commentary field as budget-withheld
- **AND** the caller can distinguish budget-withheld from absent (no entry exists)
