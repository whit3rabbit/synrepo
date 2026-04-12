## 1. Durable spec updates

- [x] 1.1 Expand `openspec/specs/repair-loop/spec.md` from the current stub to the concrete `check` / `sync` contract captured in `changes/repair-loop-v1/specs/repair-loop/spec.md`
- [x] 1.2 Validate the change artifacts with `openspec validate repair-loop-v1 --strict --type change`

## 2. Repair finding and audit model

- [x] 2.1 Add a new repair module under `src/pipeline/` that defines stable repair-surface, drift-class, repair-action, finding, report, and sync-summary types
- [x] 2.2 Add append-only resolution-log writing under `.synrepo/state/` for mutating sync runs, including source revision, requested scope, findings considered, actions taken, and final outcome
- [x] 2.3 Add unit tests covering finding serialization, stable string identifiers, and resolution-log append behavior

## 3. Read-only `synrepo check` surface

- [x] 3.1 Add `Check` to the CLI command enum in `src/bin/cli.rs` and a handler in `src/bin/cli_support/commands.rs`
- [x] 3.2 Implement the first check pass by composing existing diagnostics and maintenance planning with repair-loop classification, rather than inventing a separate health model
- [x] 3.3 Classify unsupported or absent surfaces explicitly so overlay and export gaps are visible without failing the command incorrectly
- [x] 3.4 Add CLI tests for clean state, actionable drift, blocked drift, unsupported surfaces, and machine-readable output

## 4. Targeted `synrepo sync` execution

- [x] 4.1 Add `Sync` to the CLI command enum in `src/bin/cli.rs` and a handler in `src/bin/cli_support/commands.rs`
- [x] 4.2 Implement deterministic sync execution by routing storage repairs through `plan_maintenance` / `execute_maintenance` and structural refreshes through `run_reconcile_pass()`
- [x] 4.3 Keep report-only findings unchanged during sync and surface them distinctly from repaired findings in the command summary and resolution log
- [x] 4.4 Add CLI and integration tests covering targeted repair, lock-conflict handling, report-only findings, and resolution-log output

## 5. Milestone 4 integrations and validation

- [x] 5.1 Integrate broken declared-link and stale-rationale checks against the human-guidance outputs established by `pattern-surface-v1`
- [x] 5.2 Update `CLAUDE.md`, `skill/SKILL.md`, and any CLI help text so the documented command surface includes `synrepo check` and `synrepo sync`
- [x] 5.3 Run `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, and `make check`
