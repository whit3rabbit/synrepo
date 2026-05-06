## ADDED Requirements

### Requirement: Expose synrepo_task_route as a read-only MCP tool
The MCP server SHALL expose `synrepo_task_route(task, path?)` as a read-only tool that classifies a plain-language task and returns deterministic routing recommendations. The response SHALL include `intent`, `confidence`, `recommended_tools`, `budget_tier`, `llm_required`, `edit_candidate`, `signals`, and `reason`.

#### Scenario: Route a context task
- **WHEN** an agent invokes `synrepo_task_route` with a broad search or codebase review task
- **THEN** the response recommends read-only context tools such as `synrepo_orient`, `synrepo_search` with compact output, `synrepo_find`, `synrepo_minimum_context`, `synrepo_risks`, or `synrepo_tests`
- **AND** the response marks LLM use as not required when structural context is sufficient

#### Scenario: Route an unsupported semantic transform
- **WHEN** an agent invokes `synrepo_task_route` with a task such as adding types, converting promises to async/await, or adding error handling
- **THEN** the response sets `llm_required = true`
- **AND** it does not report a deterministic edit candidate

#### Scenario: Route a conservative edit candidate
- **WHEN** an agent invokes `synrepo_task_route` with a supported mechanical edit intent
- **THEN** the response includes `edit_candidate.intent`
- **AND** the recommended tools still require `synrepo_prepare_edit_context` and `synrepo_apply_anchor_edits` for mutation

### Requirement: Keep task routing content-free and read-only
Task routing SHALL NOT run shell commands, reconcile the repository, start watch, refresh commentary, write source files, write overlay content, or persist the task text. Any metrics recorded for routing SHALL be aggregate counters only.

#### Scenario: Task route records metrics
- **WHEN** `synrepo_task_route` is invoked against a prepared repository
- **THEN** context metrics increment aggregate route counters
- **AND** the metrics do not store the task, path, prompt, source text, caller identity, or response body
