## MODIFIED Requirements

### Requirement: Use context artifact terminology without changing workflow
Agent-facing doctrine SHALL describe `synrepo_ask` as the default broad task-context front door while preserving bounded escalation through orient, drill-down, impact or risks, tests, edits, and changed-context review.

#### Scenario: Agent reads generated guidance
- **WHEN** an agent reads `skill/SKILL.md` or generated shim doctrine
- **THEN** the guidance tells the agent to prefer `synrepo_ask` for broad plain-language task packets
- **AND** it still tells the agent to use exact search/cards/minimum context for drill-down
- **AND** it still treats overlay commentary and proposed links as advisory
