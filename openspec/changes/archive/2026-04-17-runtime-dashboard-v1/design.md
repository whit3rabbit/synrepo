## Context

synrepo today exposes its operational surface through a set of non-interactive subcommands: `init`, `setup`, `status`, `watch` (foreground or `--daemon`), `reconcile`, `sync`, `check`, `export`, `upgrade`, and `agent-setup`. All of the signals that an operator cares about are already computed somewhere:

- `src/bin/cli_support/commands/status.rs` already computes reconcile health, watch summary, writer lock state, export freshness, commentary coverage, and overlay cost.
- `src/pipeline/watch/{lease,control,service}.rs` already own long-lived watch ownership, the control socket, and foreground/daemon runtime.
- `src/bin/cli_support/commands/setup.rs` already does init + shim write + MCP registration.
- `src/bootstrap/` already owns first-run layout decisions and mode detection.
- `src/store/compatibility/` already evaluates store version skew and required actions.

What is missing is a single entry experience that (a) detects runtime readiness, (b) routes the operator to the right experience, and (c) presents operational state interactively without requiring the operator to remember five subcommands.

Bare `synrepo` today prints clap help. That is a wasted opportunity: the operator has already decided they want to work with this repo.

## Goals / Non-Goals

**Goals:**
- Classify runtime state into three buckets (`uninitialized`, `partial`, `ready`) with a crisp, deterministic probe.
- Distinguish **required runtime readiness** (config, store layout, compat evaluation, status renderable) from **optional agent-integration readiness** (agent shim present, MCP registered in an external agent client).
- Route bare `synrepo` through the probe to: dashboard (ready), guided setup wizard (uninitialized), or guided repair wizard (partial). A partial install MUST NOT be treated as greenfield.
- Add a dark-theme TUI dashboard that consumes **shared** status/watch/setup models. The CLI renderer and dashboard both use the same snapshot.
- Reuse existing control-plane primitives (watch start/stop, reconcile now) rather than reimplementing them inside the TUI.
- Keep every existing subcommand and its behavior intact.

**Non-Goals:**
- Replacing the existing CLI commands or their output formats.
- Hosting the stdio MCP server inside the dashboard session. The dashboard MUST NOT bind stdin/stdout while running `synrepo mcp`.
- Multi-theme support in v1. Ship one dark palette plus `--no-color`.
- Making agent-shim presence or external MCP registration mandatory for core repo operation.
- Remote / multi-repo dashboards. V1 operates on exactly one repo (current working directory or `--repo`).
- Introducing any new runtime truth source. The dashboard is a viewer + action dispatcher.

## Decisions

### D1. Probe lives in `bootstrap/`, not `tui/`
The classification logic (uninitialized / partial / ready) is a runtime-layout concern, not a UI concern. Placing `runtime_probe.rs` under `src/bootstrap/` keeps it callable from the CLI entrypoint without dragging in ratatui/crossterm, and reuses `bootstrap::init`, `store::compatibility`, and the status snapshot helpers directly.

**Alternative considered:** Put the probe inside `src/tui/probe.rs`. Rejected because bare `synrepo` must be able to return a non-TUI error when running in a dumb terminal or over a pipe.

### D2. Partial install routes to repair, never to init
If `.synrepo/` exists but is incomplete (missing `config.toml`, missing/corrupt graph DB, compat-blocked store, missing reconcile state), the probe emits `partial` with a list of missing pieces. Routing MUST open the repair wizard, which fixes in place. Routing MUST NOT delete or reinitialize existing state without explicit user confirmation.

**Why:** A partial install is a recovery case. Treating it as greenfield risks destroying user overlay data, prior reconcile state, or watch artifacts.

### D3. Agent integration is orthogonal to runtime readiness
Missing agent shim or missing external MCP registration is surfaced as "Agent integration incomplete" in the dashboard header, not as `partial`. The dashboard still opens; core repo ops still work. A dedicated quick action opens the integration wizard.

**Why:** Runtime readiness is about whether synrepo can answer queries. Agent readiness is about whether an external client (Claude Code, Cursor, Codex, Copilot, Windsurf) can find it. Conflating them would make every fresh clone feel broken.

### D4. Dashboard MUST NOT host the stdio MCP server
`synrepo mcp` owns stdin/stdout for JSON-RPC framing. Ratatui owns stdin/stdout for the alt-screen, raw-mode terminal. Running both in one process deadlocks or corrupts both. The dashboard only shows MCP readiness and generates registration instructions; the agent client is responsible for spawning `synrepo mcp --repo <path>`.

**Alternative considered:** Spawn `synrepo mcp` on a Unix socket from the dashboard. Rejected — socket transport is not the shipped MCP surface, and adding it to scope this change is a scope violation.

### D5. Shared status snapshot model
Extract a structured `StatusSnapshot` from `src/bin/cli_support/commands/status.rs`. The CLI formatter and the TUI consume the same struct. The CLI renderer becomes a function over `StatusSnapshot`. This prevents drift between what `synrepo status` prints and what the dashboard shows, and makes dashboard tests cheap (construct a snapshot, render, assert).

### D6. TUI stack: `ratatui` + `crossterm`
Pure-Rust, Tokio-free, widely used, MIT. Both crates are small and stable enough for a v1 dashboard. No alternatives considered at this scope.

### D7. Single built-in dark palette, `--no-color` escape
Theming is in scope for v2 if needed. Pick one palette now: near-black background, muted slate borders, green healthy, amber stale, red blocked/error, cyan active/watch-live, magenta agent/MCP accent. `--no-color` disables all styling (for pipes, dumb terminals, CI).

### D8. New `src/tui/` module tree
Do not stuff TUI logic into `bin/cli_support/commands/`. The TUI is large enough to warrant its own tree:

```
src/tui/
  mod.rs          // public entry: run_dashboard(repo, opts), run_setup_wizard, run_repair_wizard
  app.rs          // App state, event loop, key handling
  theme.rs        // dark palette, color tokens, no-color mode
  probe.rs        // thin adapter over bootstrap::runtime_probe for TUI consumption
  dashboard.rs    // dashboard layout and pane wiring
  wizard.rs       // setup/repair wizard state machines
  actions.rs      // dispatch to watch/reconcile/setup primitives
  widgets/        // header, health pane, activity pane, next-actions, quick-actions, log pane
```

### D9. `setup.rs` decomposition
Refactor `src/bin/cli_support/commands/setup.rs` into named steps the wizard can call individually:
- `step_init(repo, mode)` → calls `bootstrap::bootstrap(repo, Some(mode))`
- `step_write_shim(repo, target)` → calls into `agent_shims`
- `step_register_mcp(repo, target)` → existing MCP registration logic
- `step_apply_integration(repo, target)` → composes write_shim + register_mcp

The existing `synrepo setup` command becomes a thin composer that runs all steps, preserving current behavior.

### D10. Routing table

| Probe outcome | Action |
|---|---|
| `uninitialized` (no `.synrepo/`) | open setup wizard |
| `partial` (missing config.toml / corrupt store / compat-blocked / missing reconcile state) | open repair wizard listing missing pieces |
| `ready` + agent integration present | open dashboard, integration status = green |
| `ready` + agent integration missing/incomplete | open dashboard, integration status = magenta "incomplete" |
| probe cannot even determine state (IO error) | fall back to clap help with explicit error |

### D11. No-subcommand behavior

`synrepo` (no args) → runs probe → routes. `synrepo --help`, `synrepo <any-existing-subcommand>` → unchanged. `synrepo dashboard` → explicit alias that runs the probe, but forces dashboard/error (not wizard routing). This lets scripts call `synrepo dashboard` and get a deterministic result.

### D12. Dashboard quick actions reuse existing control plane

- "Start watch" → `synrepo::pipeline::watch::control::start_foreground_or_daemon(...)` (TBD exact fn, behind a dashboard-facing adapter in `tui/actions.rs`).
- "Stop watch" → `watch::control::stop(...)`.
- "Reconcile now" → `watch::control::reconcile_now_or_local_fallback(...)` (delegates to watch owner if active, else runs a local reconcile pass).
- "Refresh" → re-run probe + re-compute `StatusSnapshot`.
- "Open setup/repair" → transitions app state machine into wizard mode.
- "Agent integration" → transitions into integration sub-wizard.

All of these MUST acquire `writer.lock` through the existing mechanism. The dashboard does not bypass locks.

### D13. Dashboard has two modes: live and poll

Bare `synrepo` and `synrepo dashboard` open the dashboard in **poll mode**: no watch service is hosted in-process; the app polls `StatusSnapshot` on a timer (every ~2s) and renders whatever watch state exists on disk (daemon active, foreground from another tty, or no watch at all). `synrepo watch` (foreground, no `--daemon`) opens the dashboard in **live mode**: the watch service runs in-process in the same code path as today's foreground watch, and its events stream into the log pane in real time.

| Invocation | Mode | Watch owner | UI |
|---|---|---|---|
| `synrepo` (bare, ready) | poll | whatever exists on disk | dashboard polling snapshot |
| `synrepo dashboard` | poll | same | same |
| `synrepo watch` | live | this process | dashboard + live log stream |
| `synrepo watch --daemon` | n/a | detached daemon | none |
| `synrepo watch --no-ui` | n/a | this process | plain log lines (scripts/CI) |
| `synrepo mcp` | n/a | n/a | stdio JSON-RPC (unchanged) |

Quick-action behavior diverges by mode: in poll mode, "Start watch (foreground)" re-execs the process in live mode or spawns a child; in live mode, "Stop watch" exits the app (the watch owner is leaving). Non-TTY `synrepo watch` auto-falls-back to `--no-ui` behavior so `synrepo watch > watch.log` keeps working.

**Alternative considered:** Always poll, never live-host. Rejected — the operator is already paying the terminal-tab cost for foreground watch, and a live event stream is what makes the dashboard feel worth having.

### D14. First-run flow: bare `synrepo` on a fresh repo goes straight to the wizard

The single most common first interaction is a user typing `synrepo` in a freshly-cloned repo. The probe returns `uninitialized` and the TUI enters the setup wizard directly — no intermediate "Do you want to set up synrepo? [y/n]" prompt. A confirmation prompt without information is a wasted click.

The wizard arc:

1. **Splash** (one screen, Enter to continue, Esc to exit without writes) — one-sentence description of what's about to happen, expected runtime, and the "nothing leaves your machine" reassurance.
2. **Mode** — radio choice between `auto` (default, highlighted) and `curated`, with a one-line caption for each. Default based on observational signals: if the repo already has concept directories populated, curated is suggested; otherwise auto.
3. **Agent target** — list of supported targets (claude, cursor, codex, copilot, windsurf, generic) plus a first-class "Skip" option. Pre-select based on observational signals (see D15).
4. **Confirm + run** — show the exact plan (e.g. "init → parse → reconcile → write `.cursor/rules/synrepo.mdc` → register MCP in Cursor"), then execute `step_init` → optional `step_write_shim` → optional `step_register_mcp` → first reconcile, with a progress bar per step.
5. **Land in dashboard** with a one-shot "Welcome" banner in the log pane.

Before the "Confirm + run" step, the wizard is pure UI state — no files are written. Cancelling is free. After confirm, a mid-run cancellation leaves `.synrepo/` in a partial state which the next `synrepo` invocation correctly routes to the repair wizard.

The repair wizard (partial state) deliberately omits splash and mode re-prompt: the user has used synrepo before. It jumps straight to "Found `.synrepo/` with missing pieces: [list]. Fix them?".

### D15. Agent-target auto-detection is observational, not mutating

The setup wizard pre-selects an agent target by reading file-system hints: `.claude/` directory or `CLAUDE.md` → claude; `.cursor/` → cursor; `.codex/` or existing codex config → codex; `.github/copilot-*` or Copilot settings → copilot; `.windsurf/` → windsurf. Fall back to the user's home-dir equivalents if the repo has none. If multiple hints match, present them in detection-order with the first highlighted.

This detection reads files; it does not touch `.synrepo/`, acquire locks, or mutate anything. The `runtime-probe` spec codifies this as a read-only contract. The user can always override the pre-selection or pick "Skip".

### D16. Non-TTY first-run: no TUI prompting over a pipe

If bare `synrepo` is invoked on a fresh repo but stdout is not a TTY (CI, piped, redirected), the binary MUST NOT enter the wizard. Instead it prints a short message telling the user to run `synrepo init` explicitly and exits non-zero. The same applies to bare `synrepo` on a partial repo: text summary + non-zero exit + pointer to `synrepo status` or `synrepo upgrade`. Only `ready` state produces a zero-exit text summary (the fallback already spec'd for the poll dashboard).

## Risks / Trade-offs

- **Risk**: The TUI hangs or panics on unusual terminals (Windows legacy console, dumb pipes, CI). **Mitigation**: detect non-TTY stdout early in `mod.rs`; if not a TTY, print a short status summary and exit zero. `--no-color` is also honored. Crossterm already handles Windows modern consoles.
- **Risk**: Dashboard drifts from `synrepo status` output. **Mitigation**: shared `StatusSnapshot` struct; CLI and TUI are both formatters over it. Add a snapshot-based test that verifies parity of key fields.
- **Risk**: Users perceive a partial install as broken because of the repair wizard. **Mitigation**: the repair wizard MUST list exactly what is missing, MUST NOT mention "new project" or "start over" language, and MUST preserve all existing state.
- **Risk**: Writer-lock contention during dashboard actions (e.g. reconcile now while watch daemon is running). **Mitigation**: reuse the existing `watch::control::reconcile_now` delegation path; surface lock conflicts in the log pane as structured events, not panics.
- **Risk**: Probe classification changes over time and breaks the routing table silently. **Mitigation**: the `runtime-probe` spec defines the classification contract; tests cover each transition.
- **Risk**: Scope creep into multi-repo or remote dashboards. **Mitigation**: v1 is locked to one repo via the existing `--repo` flag. Non-goal explicitly stated.
- **Trade-off**: Adding `ratatui` + `crossterm` increases binary size and compile time. Accepted cost — they are small, well-maintained, and the operator UX gain is significant. Both are pure Rust with no C deps.
- **Trade-off**: `synrepo` with no args changes from printing clap help to launching a TUI. This is a visible behavior change. Mitigation: non-TTY stdout falls back to a short text summary, so scripts and CI are not disrupted.
