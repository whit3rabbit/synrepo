## ADDED Requirements

### Requirement: Compile task contexts from deterministic recipes
synrepo SHALL provide a runtime context planning layer that compiles a high-level ask into bounded code artifact targets without changing canonical graph extraction. The planner SHALL support built-in recipes for `explain_symbol`, `trace_call`, `review_module`, `security_review`, `release_readiness`, `fix_test`, and `general`.

#### Scenario: Ask includes explicit scope
- **WHEN** a request includes `scope.paths` or `scope.symbols`
- **THEN** synrepo maps those scopes to existing context artifact target kinds such as `file`, `directory`, `symbol`, `minimum_context`, or `call_path`
- **AND** target count and token budget controls are applied before rendering

#### Scenario: Ask has no explicit scope
- **WHEN** a request includes only plain-language `ask` text
- **THEN** synrepo falls back to bounded search artifacts derived from the recipe and ask text
- **AND** the response records omitted or downgraded context rather than returning unbounded raw source

### Requirement: Preserve trust boundaries in task-context packets
Task-context planning SHALL preserve the graph/overlay trust boundary. Graph-backed and substrate-backed artifacts remain the default. Overlay notes and commentary SHALL be excluded unless the request explicitly allows overlay inclusion.

#### Scenario: Overlay is not allowed
- **WHEN** `ground.allow_overlay` is absent or false
- **THEN** the task-context packet excludes advisory overlay notes and commentary
- **AND** the response records an omitted-context note explaining that overlay content was excluded

#### Scenario: Overlay is allowed
- **WHEN** `ground.allow_overlay` is true
- **THEN** synrepo may include advisory overlay notes through existing context-pack note attachment
- **AND** overlay output remains advisory and does not become graph truth
