## 1. Dependencies and scaffolding

- [x] 1.1 Add `ratatui` and `crossterm` as workspace dependencies in `Cargo.toml` with pinned minor versions; verify they build on macOS, Linux, and Windows CI matrices
- [x] 1.2 Create empty `src/tui/` module tree (`mod.rs`, `app.rs`, `theme.rs`, `probe.rs`, `dashboard.rs`, `wizard.rs`, `actions.rs`, `widgets/mod.rs`) with `pub mod tui;` wired in `src/lib.rs`
- [x] 1.3 Confirm the no-TTY / `--no-color` fallback path compiles without pulling the terminal into raw mode in a dumb environment

## 2. Runtime probe (required-readiness classification)

- [x] 2.1 Add `src/bootstrap/runtime_probe.rs` exposing `RuntimeClassification` (`Uninitialized`, `Partial { missing: Vec<Missing> }`, `Ready`) and `probe(repo: &Path) -> ProbeReport`
- [x] 2.2 Implement required-component checks (config.toml readable, store layout present, compat evaluation non-blocking, status snapshot producible) reusing existing helpers in `src/bootstrap/init/`, `src/store/compatibility/`, and `src/bin/cli_support/commands/status.rs`
- [x] 2.3 Guarantee probe is read-only: no writer-lock acquisition, no store mutation, no JSONL append. Add a test that asserts file-system and lock state are bit-identical before and after a probe run
- [x] 2.4 Add unit tests covering the three classifications plus edge cases (readable-but-corrupt config, compat-blocked store, missing graph DB, missing reconcile-state.json)

## 3. Agent-integration readiness (supplementary signal)

- [x] 3.1 Extend `ProbeReport` with `AgentIntegration { Absent, Partial { target }, Complete { target } }`
- [x] 3.2 Detect shim presence via the existing target-path map in `src/bin/cli_support/agent_shims/`; detect MCP registration presence via the existing setup helpers in `src/bin/cli_support/commands/setup.rs`
- [x] 3.3 Add unit tests for each integration state across all supported targets (claude, cursor, copilot, generic, codex, windsurf)

## 4. Routing decision

- [x] 4.1 Add `RoutingDecision` enum in `src/bootstrap/runtime_probe.rs` mapping classification to `OpenSetup`, `OpenRepair { missing }`, `OpenDashboard { integration }`, or `FallbackHelp { error }`
- [x] 4.2 Add tests asserting partial state always routes to `OpenRepair` and never to `OpenSetup`

## 5. CLI entrypoint wiring

- [x] 5.1 Add `dashboard` subcommand to `src/bin/cli_support/cli_args.rs` (explicit alias; opens poll-mode dashboard or exits non-zero on non-ready state)
- [x] 5.2 Allow bare `synrepo` with no subcommand in clap config; route via `bootstrap::runtime_probe::probe` + `RoutingDecision` in `src/bin/cli.rs`
- [x] 5.3 Add a non-TTY fallback in the bare-entry path: on `ready` print a concise text summary and exit zero; on `uninitialized` or `partial` print an instructional message and exit non-zero
- [x] 5.4 Add a `--no-ui` (or `--log`) flag to `synrepo watch` for explicit plain-log foreground operation; wire auto-detection so non-TTY foreground `synrepo watch` also falls back to plain logs
- [x] 5.5 Wire `synrepo watch` in a TTY (no `--daemon`, no `--no-ui`) to open the dashboard in live mode hosting the watch service in-process
- [x] 5.6 Preserve every existing subcommand's exact behavior; add a CLI smoke test asserting `synrepo status`, `synrepo init`, `synrepo watch --daemon`, `synrepo watch --no-ui`, `synrepo sync`, `synrepo check`, `synrepo export`, `synrepo upgrade`, `synrepo agent-setup`, `synrepo mcp` are all still dispatched unchanged

## 6. Shared status snapshot model

- [x] 6.1 Extract a `StatusSnapshot` struct from `src/bin/cli_support/commands/status.rs` covering repo path, mode, watch state, reconcile health, writer-lock state, export freshness, commentary coverage, overlay cost, and agent-integration signal
- [x] 6.2 Refactor the existing `synrepo status` renderer to be a pure function over `StatusSnapshot` — no behavior change in text or `--json` output
- [x] 6.3 Add a parity test: `StatusSnapshot` → CLI text and `StatusSnapshot` → dashboard rendering must agree on key fields (watch mode, reconcile health, export freshness, overlay cost)

## 7. Setup decomposition

- [x] 7.1 Refactor `src/bin/cli_support/commands/setup.rs` into named steps: `step_init(repo, mode)`, `step_write_shim(repo, target)`, `step_register_mcp(repo, target)`, `step_apply_integration(repo, target)`
- [x] 7.2 Make the existing `synrepo setup` command a thin composer that runs all steps in the prior order; preserve CLI output exactly
- [x] 7.3 Add unit tests covering each step in isolation, including idempotent re-runs

## 8. Dashboard shell

- [x] 8.1 Implement the dark palette in `src/tui/theme.rs` with `Theme::dark()` and `Theme::plain()` variants and semantic color tokens (healthy, stale, blocked, watch-active, agent-accent)
- [x] 8.2 Implement the app shell in `src/tui/app.rs`: event loop, key handling, state machine for Dashboard(poll|live) / SetupWizard / RepairWizard / IntegrationWizard modes
- [x] 8.3 Implement the header widget surfacing repo path, mode, watch state, reconcile health, writer-lock state, and MCP readiness
- [x] 8.4 Implement the system-health pane reading `StatusSnapshot`
- [x] 8.5 Implement the recent-activity pane reusing the existing bounded recent-activity surface
- [x] 8.6 Implement the next-actions pane showing recommended actions derived from health signals (stale export, stale commentary, compat advisory, missing integration)
- [x] 8.7 Implement the quick-actions pane with key bindings for start/stop watch, reconcile now, refresh, open setup/repair, open integration; diverge per dashboard mode (poll vs live)
- [x] 8.8 Implement the event/notification log pane backed by a bounded in-memory ring buffer; in live mode pipe watch-service events into the same buffer
- [x] 8.9 Implement the poll-mode refresh loop (periodic `StatusSnapshot` recomputation, ~2s cadence) and the live-mode event-stream subscription to the in-process watch service
- [x] 8.10 Implement the stop-watch-in-live-mode behavior: action stops the watch service and exits the dashboard process

## 9. Operational actions

- [x] 9.1 Implement `src/tui/actions.rs` dispatchers that call into existing `src/pipeline/watch/control.rs` (start, stop, reconcile-now) and the decomposed setup steps
- [x] 9.2 Surface writer-lock and watch-ownership conflicts as structured log entries in the dashboard log pane (owner PID, acquired-at timestamp)
- [x] 9.3 Ensure every mutating action acquires the writer lock via the existing mechanism; add a test that exercises a lock-contended reconcile-now and asserts the dashboard does not panic

## 10. Setup wizard

- [x] 10.1 Implement mode selection and optional agent-target selection in `src/tui/wizard.rs`
- [x] 10.2 Wire the wizard to `step_init` → optional `step_write_shim` → optional `step_register_mcp` → initial reconcile, with a cancel path that performs no writes
- [x] 10.3 Transition to dashboard on successful completion; add tests for happy path, mid-flow cancel, and init failure

## 10a. First-run UX polish

- [x] 10a.1 Implement the splash step in `src/tui/wizard.rs` — one screen with a one-sentence description, estimated runtime, privacy reassurance, Enter-to-continue, Esc-to-exit
- [x] 10a.2 Add observational agent-target detection helpers in `src/bootstrap/runtime_probe.rs` (repo and `$HOME` hints for claude, cursor, codex, copilot, windsurf); return a deterministic ordered list
- [x] 10a.3 Wire the wizard agent-target step to pre-highlight the first detected target; always include "Skip" as a first-class choice
- [x] 10a.4 Wire mode-selection default based on observational signals (concept directories present → suggest curated; otherwise auto)
- [x] 10a.5 Implement the plan-confirmation step that renders the exact planned actions (init → parse → reconcile → optional shim path → optional MCP target) before any writes
- [x] 10a.6 Guarantee no file-system writes before the "Confirm + run" step; add a test asserting that wizard cancellation at the splash, mode, and agent-target steps leaves the working directory byte-identical
- [x] 10a.7 Implement the one-shot welcome banner in the dashboard log pane on first successful transition from the wizard
- [x] 10a.8 Implement the non-TTY first-run fallback: on `uninitialized` or `partial` state with non-TTY stdout, print a short instructional message and exit non-zero without entering the TUI
- [x] 10a.9 Make the repair wizard skip splash and mode re-prompt; open directly at the missing-components list

## 11. Repair wizard

- [x] 11.1 Render the structured `missing` list from the probe report; group by required-runtime vs. optional-agent integration
- [x] 11.2 Wire repair actions: write default `config.toml`, run `synrepo upgrade --apply` (only with explicit user confirmation) for compat-blocked stores, run reconcile, write missing agent shims
- [x] 11.3 Guarantee no destructive actions without explicit user confirmation; add a test asserting that cancelling the repair wizard leaves `.synrepo/` byte-identical
- [x] 11.4 Re-run the probe after each repair step; transition to dashboard when classification becomes `ready`

## 12. Agent-integration completion flow

- [x] 12.1 Add an integration sub-wizard launched from the dashboard quick action
- [x] 12.2 Detect the configured target (or prompt to choose one) and offer to write the shim, register MCP, or both based on the current integration state
- [x] 12.3 Never overwrite an existing shim unless the user passes a `--regen` equivalent confirmation within the wizard

## 13. Non-TTY / no-color fallback

- [x] 13.1 Detect non-TTY stdout in `src/tui/mod.rs` before entering the alternate screen
- [x] 13.2 Print a concise text summary (probe classification, key health signals, recommended next subcommand) and exit zero
- [x] 13.3 Honor `--no-color` as a plain-rendering mode that still enters the TUI but disables styling; add tests for both pipe-out and `--no-color` paths

## 14. Documentation and roadmap

- [x] 14.1 Update `AGENTS.md` / `CLAUDE.md` Commands section to document bare `synrepo` and `synrepo dashboard`
- [x] 14.2 Add a "Phase N — Runtime UX and Operator Surface" entry to `ROADMAP.md` pointing at this change *(ROADMAP.md was removed in commit 26960e7; Phase 7 entry added to `docs/FOUNDATION-SPEC.md` §17 instead — this is the current phased-plan home)*
- [x] 14.3 Add a short "Interactive surface" section to `docs/FOUNDATION-SPEC.md` linking to the runtime-probe and dashboard specs

## 15. Validation and archive readiness

- [x] 15.1 Run `openspec validate runtime-dashboard-v1 --strict` and address any reported issues
- [x] 15.2 Run `make check` (fmt-check + clippy + tests) and confirm green on the CI matrix
- [ ] 15.3 Manual smoke test on each supported platform: bare `synrepo` on uninitialized, partial, and ready repos; verify non-TTY fallback via `synrepo | cat` *(left to the operator preparing the release; the non-TTY path is covered by `tui::pipe_out_*` unit tests but cross-platform smoke is manual by definition)*
- [ ] 15.4 Prepare the archive checklist per `opsx:archive` conventions *(run `/opsx:archive runtime-dashboard-v1` when ready to move the change under `openspec/changes/archive/`)*
