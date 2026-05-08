## ADDED Requirements

### Requirement: Guide Agents To Resume Context Before Repetition
Agent-facing doctrine SHALL tell agents to use the explicit repo resume context surface after stale resumes or context loss before asking the user to repeat repository state.

#### Scenario: Agent resumes stale repo work
- **WHEN** an agent needs to continue prior repo work but local conversation context is insufficient
- **THEN** doctrine guides the agent to call `synrepo_resume_context`
- **AND** the guidance does not imply synrepo stores generic chat or session memory
