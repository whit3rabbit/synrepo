## ADDED Requirements

### Requirement: Count Resume Context Responses Without Content
Context accounting SHALL track repo resume context responses using aggregate counters only. Persisted metrics SHALL NOT store prompts, response bodies, note claims, caller identity, raw tool output, or session history.

#### Scenario: Resume packet is served
- **WHEN** synrepo serves a repo resume context packet
- **THEN** context metrics record the response count and token estimate
- **AND** the persisted metrics contain no packet body or note claim text
