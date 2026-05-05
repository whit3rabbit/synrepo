## ADDED Requirements

### Requirement: Treat deterministic edit candidates as recommendations only
Deterministic edit candidates SHALL be advisory route results. They SHALL NOT mutate source by themselves. Source mutation SHALL continue to require `synrepo_prepare_edit_context` followed by `synrepo_apply_anchor_edits` on an MCP server started with `--allow-source-edits`.

#### Scenario: Hook detects a deterministic edit candidate
- **WHEN** a hook classifies a task as `var-to-const`, `remove-debug-logging`, `replace-literal`, or `rename-local`
- **THEN** the hook may emit `[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE]`
- **AND** no source file is written by the hook

#### Scenario: Ambiguous transform is requested
- **WHEN** the task requires semantic inference beyond local parser proof
- **THEN** the route result does not claim a deterministic edit is eligible
- **AND** the task may be marked LLM-required

### Requirement: Prove TypeScript var-to-const eligibility conservatively
The TypeScript/TSX `var-to-const` eligibility helper SHALL report eligible only when it can identify a single `var` or `let` declaration and prove there is no later reassignment to that binding in the inspected source snippet. Ambiguous snippets SHALL be ineligible.

#### Scenario: Variable is never reassigned
- **WHEN** the helper inspects `let value = 1; console.log(value);`
- **THEN** it reports `eligible = true`

#### Scenario: Variable is reassigned
- **WHEN** the helper inspects `let value = 1; value = 2;`
- **THEN** it reports `eligible = false`
