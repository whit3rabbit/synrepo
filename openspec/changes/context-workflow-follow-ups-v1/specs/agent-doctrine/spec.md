## MODIFIED Requirements

### Requirement: Teach the context workflow
Agent-facing doctrine SHALL teach the workflow as orient, find cards, inspect impact (via `synrepo_impact` or its shorthand `synrepo_risks`), edit, validate tests, and check changed context.

#### Scenario: Agent reads generated instructions
- **WHEN** an agent reads the generated synrepo doctrine or skill file
- **THEN** the instructions tell the agent to start with synrepo context before large cold file reads
- **AND** the instructions identify the workflow aliases (including `synrepo_risks` as a shorthand for `synrepo_impact`) and the budget escalation rule
