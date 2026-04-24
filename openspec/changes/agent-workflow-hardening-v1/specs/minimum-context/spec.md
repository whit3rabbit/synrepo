## ADDED Requirements

### Requirement: Position minimum-context as the bounded neighborhood step
`synrepo_minimum_context` SHALL be documented and described as the bounded neighborhood step agents use before deep inspection when a focal target is known.

#### Scenario: Agent has a likely edit target
- **WHEN** an agent has identified a file or symbol through orient, find, or explain
- **THEN** workflow guidance directs the agent to use minimum-context or impact/risk tools before broad source inspection
- **AND** the response keeps its existing graph-backed and git-labeled trust boundaries

#### Scenario: Minimum context is insufficient
- **WHEN** a minimum-context response lacks enough implementation detail for a necessary edit
- **THEN** the workflow allows explicit escalation to a deep card or full-file read
- **AND** that escalation is treated as intentional rather than the default first step
