## 1. Context Accounting Core

- [x] 1.1 Add shared `ContextAccounting` card metadata and token-estimation helpers.
- [x] 1.2 Populate accounting metadata for symbol, file, change-risk, test-surface, module, entrypoint, public-api, call-path, and neighborhood card responses.
- [x] 1.3 Preserve existing `approx_tokens` fields for compatibility.

## 2. Operational Metrics

- [x] 2.1 Add `.synrepo/state/context-metrics.json` load/save helpers with best-effort recording.
- [x] 2.2 Record card served counts, token estimates, raw-file estimates, savings, budget tier usage, truncation, latency, stale counts, changed-file counts, and test-surface hits.
- [x] 2.3 Surface context metrics through `synrepo status --json` and dashboard shared status snapshot.

## 3. CLI And MCP Workflow

- [x] 3.1 Add CLI aliases: `cards`, `explain`, `impact`, `tests`, `risks`, and `stats context`.
- [x] 3.2 Add `bench context --tasks <glob> --json` with deterministic task-fixture reporting.
- [x] 3.3 Add MCP workflow aliases: `synrepo_orient`, `synrepo_find`, `synrepo_explain`, `synrepo_impact`, `synrepo_tests`, and `synrepo_changed`.
- [x] 3.4 Accept optional numeric caps on card-set workflow entry points and mark truncation in accounting.

## 4. Doctrine And Documentation

- [x] 4.1 Update `skill/SKILL.md`, README, and canonical agent doctrine with the orient-find-impact-edit-tests-changed loop.
- [x] 4.2 Record `overlay-agent-notes-v1` as explicit follow-up work, not part of this implementation.

## 5. Verification

- [x] 5.1 Add unit and snapshot coverage for accounting metadata on key card types and MCP output.
- [x] 5.2 Add CLI dispatch tests for aliases and numeric caps.
- [x] 5.3 Add benchmark fixture tests with deterministic hit/miss and token reporting.
- [x] 5.4 Run OpenSpec status plus focused cargo tests for changed surfaces.
