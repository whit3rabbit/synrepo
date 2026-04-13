## ADDED Requirements

### Requirement: Expose synrepo_entrypoints as a task-first MCP tool
synrepo SHALL expose a `synrepo_entrypoints(scope?, budget?)` MCP tool that returns an `EntryPointCard` for the requested scope. The `scope` parameter SHALL be an optional path prefix string; when absent, the compiler scans all indexed files. The `budget` parameter SHALL accept `"tiny"` (default), `"normal"`, or `"deep"`. Results SHALL be sorted by kind (binary first, then cli_command, http_handler, lib_root) then by file path within each kind. The result set SHALL be limited to 20 entries by default. The tool SHALL return a parseable JSON object and SHALL NOT raise an error when no entry points are found — it returns an empty `entry_points` list instead.

#### Scenario: Agent requests entry points with no scope
- **WHEN** an agent invokes `synrepo_entrypoints` without a `scope` argument
- **THEN** the tool returns an `EntryPointCard` covering all indexed files
- **AND** results are sorted binary-first then by file path
- **AND** the result count is at most 20

#### Scenario: Agent requests entry points scoped to a directory
- **WHEN** an agent invokes `synrepo_entrypoints` with `scope: "src/bin/"`
- **THEN** only entry points whose file paths start with `src/bin/` are returned
- **AND** entry points from other directories are excluded

#### Scenario: No entry points found in scope
- **WHEN** `synrepo_entrypoints` is called with a `scope` that has no matching entry points
- **THEN** the tool returns a JSON object with an empty `entry_points` array
- **AND** no error or non-zero exit status is produced

#### Scenario: Tool respects budget parameter
- **WHEN** `synrepo_entrypoints` is called with `budget: "normal"`
- **THEN** each entry in the response includes the caller count and truncated doc comment
- **AND** source bodies are omitted
