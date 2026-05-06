## 1. OpenSpec Artifacts

- [x] 1.1 Create `proposal.md` for the context artifact framing change.
- [x] 1.2 Create `design.md` documenting framing decisions and explicit non-goals.
- [x] 1.3 Add delta specs for `context-artifacts`, `foundation`, `cards`, `mcp-surface`, and `agent-doctrine`.

## 2. Documentation Framing

- [x] 2.1 Update `docs/FOUNDATION.md` to describe `repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP`.
- [x] 2.2 Update `README.md`, `docs/ARCHITECTURE.md`, and `docs/MCP.md` with the same vocabulary and no runtime behavior changes.

## 3. Agent Guidance

- [x] 3.1 Update `skill/SKILL.md` to describe cards and context packs as artifact/context delivery surfaces.
- [x] 3.2 Update `src/surface/agent_doctrine.rs` terminology without changing workflow rules.

## 4. Validation

- [x] 4.1 Run `openspec validate context-artifacts-framing-v1 --strict`.
- [x] 4.2 Run `cargo test --bin synrepo agent_shims` and `cargo test --bin synrepo mcp_schema`.
- [x] 4.3 Run a doc sanity search for the new framing terms.
