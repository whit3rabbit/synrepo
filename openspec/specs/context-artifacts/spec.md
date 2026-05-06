# context-artifacts Specification

## Purpose
TBD - created by archiving change context-artifacts-framing-v1. Update Purpose after archive.
## Requirements
### Requirement: Define context artifact layers
synrepo SHALL define its context product model as `repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP`. Graph facts SHALL be canonical observed facts. Code artifacts SHALL be deterministic compiled records derived from graph, substrate, git, and approved human-authored inputs. Task contexts SHALL be bounded bundles of artifacts for a workflow. Cards and MCP responses SHALL be delivery packets for those artifacts and contexts.

#### Scenario: Contributor describes the product model
- **WHEN** a contributor updates product docs, agent guidance, or OpenSpec text
- **THEN** the text can distinguish graph facts, code artifacts, task contexts, and cards/MCP delivery
- **AND** it does not collapse overlay output into canonical graph facts

### Requirement: Keep framing separate from runtime storage
The context-artifact framing SHALL NOT imply a persistent artifact registry, new storage table, new cache, or new invalidation workflow unless a later change explicitly specifies one.

#### Scenario: Framing-only change is implemented
- **WHEN** this change is applied
- **THEN** no runtime storage path, SQLite schema, JSONL artifact cache, or migration is added
- **AND** existing cards and context packs remain the delivery mechanisms

### Requirement: Preserve graph and overlay trust boundaries
Context artifact language SHALL preserve the existing trust model: graph-backed facts remain authoritative, while overlay commentary, proposed links, explain docs, and agent notes remain advisory and freshness-labeled.

#### Scenario: Artifact text references overlay content
- **WHEN** docs describe a task context that includes advisory overlay content
- **THEN** the docs label that content as overlay-backed and advisory
- **AND** graph-backed facts remain the canonical source for code truth

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

