## ADDED Requirements

### Requirement: Expose synrepo_ask as a bounded task-context MCP tool
synrepo SHALL expose `synrepo_ask(ask, scope?, shape?, ground?, budget?)` as a default read-only MCP tool. The tool SHALL compile the request into existing context-pack targets and return a JSON object containing `schema_version`, `ask`, `recipe`, `answer`, `cards_used`, `evidence`, `grounding`, `budget`, `omitted_context_notes`, `next_best_tools`, and `context_packet`.

#### Scenario: Agent asks for a scoped module review
- **WHEN** an agent invokes `synrepo_ask` with `ask: "review this module"` and `scope.paths: ["src/surface/mcp"]`
- **THEN** the response includes a bounded `context_packet`
- **AND** `cards_used` lists the rendered artifacts
- **AND** `next_best_tools` recommends drill-down tools

#### Scenario: Grounding requires citations
- **WHEN** an agent invokes `synrepo_ask` with `ground.mode = "required"` or `ground.citations = "required"`
- **THEN** the response includes `grounding.status`
- **AND** the response includes `evidence` entries for rendered artifacts when source fields are available
- **AND** missing spans are represented as `null` rather than fabricated line ranges

#### Scenario: Tool remains read-only
- **WHEN** `synrepo_ask` is invoked
- **THEN** it SHALL NOT mutate source files, graph facts, overlay notes, commentary, or external process state
- **AND** it MAY update existing best-effort MCP/context metrics
