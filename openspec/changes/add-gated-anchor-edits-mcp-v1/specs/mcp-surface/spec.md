## ADDED Requirements

### Requirement: Gate MCP mutation behind explicit process invocation
The MCP server SHALL remain read-first by default. Mutating MCP tools SHALL be registered only when the server is started with an explicit process-level edit gate such as `synrepo mcp --allow-edits`. Configuration MAY further restrict edit capability, but configuration alone SHALL NOT enable mutating MCP tools.

#### Scenario: Default MCP does not advertise edit tools
- **WHEN** a user starts `synrepo mcp` without `--allow-edits`
- **AND** an MCP client lists tools
- **THEN** `synrepo_prepare_edit_context` is absent
- **AND** `synrepo_apply_anchor_edits` is absent
- **AND** existing read-first tools remain available

#### Scenario: Edit-enabled MCP advertises edit tools
- **WHEN** a user starts `synrepo mcp --allow-edits`
- **AND** policy does not further disable editing
- **AND** an MCP client lists tools
- **THEN** `synrepo_prepare_edit_context` is present
- **AND** `synrepo_apply_anchor_edits` is present
- **AND** each tool description states that it can lead to source file mutation only through the apply tool

#### Scenario: Config cannot silently enable edits
- **WHEN** repository or user configuration permits edit-capable MCP behavior
- **AND** the server is started as `synrepo mcp` without `--allow-edits`
- **THEN** mutating tools are not registered
- **AND** calling either edit tool by name returns a not-available error

### Requirement: Expose a prepare/apply anchored edit workflow
When edit mode is enabled, synrepo SHALL expose a two-step MCP workflow: `synrepo_prepare_edit_context` for preparing anchored source context and `synrepo_apply_anchor_edits` for validated source mutation. The apply tool SHALL require freshness inputs produced by prepare, including `task_id`, `anchor_state_version`, `path`, `content_hash`, `anchor`, optional `end_anchor`, `edit_type`, and `text`.

#### Scenario: Agent prepares and applies a single-file edit
- **WHEN** edit mode is enabled
- **AND** an agent calls `synrepo_prepare_edit_context` for a file target
- **THEN** the response includes a task ID, anchor state version, path, content hash, and prepared anchors
- **WHEN** the agent calls `synrepo_apply_anchor_edits` with those freshness fields and replacement text
- **THEN** synrepo validates the anchors against current file content before writing
- **AND** the response reports the per-file edit outcome and post-edit diagnostics

#### Scenario: Apply without prepare is rejected
- **WHEN** edit mode is enabled
- **AND** an agent calls `synrepo_apply_anchor_edits` with an unknown `task_id` or `anchor_state_version`
- **THEN** synrepo rejects the edit as stale or unprepared
- **AND** no source file is modified

#### Scenario: Command execution remains unavailable
- **WHEN** edit mode is enabled
- **AND** an MCP client lists tools
- **THEN** no arbitrary command execution tool is registered as part of this workflow
