## 1. Contracts and Helpers

- [x] 1.1 Add shared compact-output types and helpers for deterministic token estimates, omitted counts, and budget trimming.
- [x] 1.2 Extend content-free context metrics for compact-output counters with backwards-compatible defaults.

## 2. MCP Runtime

- [x] 2.1 Add `output_mode` and `budget_tokens` handling to `synrepo_search` while preserving default output.
- [x] 2.2 Add `output_mode` handling to `synrepo_context_pack` and compact search artifacts only.
- [x] 2.3 Update MCP tool descriptions and server instructions to describe compact output and card escalation.

## 3. Docs and Validation

- [x] 3.1 Update `docs/MCP.md` and `skill/SKILL.md` with compact output guidance.
- [x] 3.2 Add tests for default search compatibility, compact search, context-pack search compaction, metrics compatibility, and content-free persistence.
- [x] 3.3 Run focused tests, lint, and OpenSpec status checks.
