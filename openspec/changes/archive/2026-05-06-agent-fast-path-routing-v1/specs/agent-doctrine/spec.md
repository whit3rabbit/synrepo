## ADDED Requirements

### Requirement: Surface deterministic fast-path routing signals
Agent-facing doctrine SHALL tell agents to prefer deterministic synrepo fast paths before cold file reads or LLM-heavy work when a task can be answered by structural cards, compact search, context packs, risk/test surfaces, or gated anchored edits.

#### Scenario: Agent sees fast-path hook output
- **WHEN** a Codex or Claude hook emits `[SYNREPO_CONTEXT_FAST_PATH]`
- **THEN** the agent guidance tells the agent to use the recommended synrepo read tools before opening full source
- **AND** the guidance does not imply the hook can block or execute tools

#### Scenario: Agent sees deterministic edit candidate
- **WHEN** a hook emits `[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: <intent>`
- **THEN** the agent guidance tells the agent to prepare anchors and apply edits only through edit-gated MCP tools when they are available
- **AND** it does not imply the hook can mutate source files

#### Scenario: Task does not require LLM output
- **WHEN** a task-route result includes `[SYNREPO_LLM_NOT_REQUIRED]`
- **THEN** doctrine tells agents to use structural context or anchored edits first
- **AND** overlay commentary remains optional and freshness-labeled
