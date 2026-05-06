## ADDED Requirements

### Requirement: Use context artifact terminology without changing workflow
Agent-facing doctrine SHALL be allowed to describe synrepo as serving graph facts, code artifacts, task contexts, and cards, but it SHALL preserve the existing workflow: orient, find, inspect impact or risks, edit, test, and review changed context with `tiny`, `normal`, and `deep` budget escalation.

#### Scenario: Agent reads generated guidance
- **WHEN** an agent reads `skill/SKILL.md` or generated shim doctrine
- **THEN** the guidance may use context artifact terminology
- **AND** it still tells the agent to start with bounded synrepo context before cold source reads
- **AND** it still treats overlay commentary and proposed links as advisory

#### Scenario: Doctrine wording changes
- **WHEN** the canonical doctrine block is edited for context artifact terminology
- **THEN** generated shims pick up the terminology through the existing shared doctrine mechanism
- **AND** no agent-facing surface introduces a competing default path
