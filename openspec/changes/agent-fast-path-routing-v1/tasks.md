## 1. Contracts

- [ ] 1.1 Add proposal, design, tasks, and spec deltas for fast-path routing.
- [ ] 1.2 Define the task-route response contract and deterministic edit boundaries.

## 2. Runtime Surface

- [ ] 2.1 Add the shared task-route classifier and TypeScript `var-to-const` eligibility helper.
- [ ] 2.2 Expose `synrepo_task_route` over MCP and `synrepo task-route` over CLI.
- [ ] 2.3 Extend agent nudge hooks with structured fast-path signals.

## 3. Metrics and UI

- [ ] 3.1 Add content-free context metrics counters for classifications, hook signals, edit candidates, anchored edit outcomes, and estimated LLM calls avoided.
- [ ] 3.2 Render the new counters in `status --json`, status text, and dashboard Health rows.

## 4. Docs and Validation

- [ ] 4.1 Update `docs/MCP.md`, `docs/FOUNDATION.md`, `skill/SKILL.md`, and canonical doctrine/shim tests.
- [ ] 4.2 Add focused tests for classification, hook signals, MCP schema, metrics, and anchored edit regressions.
- [ ] 4.3 Run focused tests plus `cargo clippy --workspace --bins --lib -- -D warnings`.
