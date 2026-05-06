## 1. Contracts

- [x] 1.1 Add OpenSpec deltas for task-context packets and `synrepo_ask`.
- [x] 1.2 Document grounding, scope, shape, and budget request controls.

## 2. Runtime

- [x] 2.1 Add `src/surface/context/` request, recipe, target, and compiler modules.
- [x] 2.2 Add `src/surface/mcp/ask.rs` and wire it to existing context-pack rendering.
- [x] 2.3 Register `synrepo_ask` as a default read-only MCP tool and rate-limit it with card/context-pack reads.

## 3. Agent Guidance and Docs

- [x] 3.1 Update `skill/SKILL.md` to default broad plain-language tasks to `synrepo_ask`.
- [x] 3.2 Update MCP docs and README with the new task-context front door.

## 4. Validation

- [x] 4.1 Add source registration/schema tests for `synrepo_ask`.
- [x] 4.2 Add focused unit tests for context planning and ask packet output.
- [x] 4.3 Run `openspec validate context-task-packets-v1 --strict`.
- [x] 4.4 Run focused MCP/context tests in a clean checkout if unrelated local TUI edits block direct cargo tests.
