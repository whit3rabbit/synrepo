## Why

Today the first-run and daily-run experience of `synrepo` is fragmented across separate subcommands (`init`, `setup`, `status`, `watch`, `sync`, `reconcile`). The substrate for a unified operator surface already exists — bootstrap/init, `setup` (init + shim + MCP registration), `status` (reconcile health, watch summary, writer lock, export freshness, commentary coverage, overlay cost), and watch lease/control — but there is no single entry point that routes users to the right experience or surfaces operational state interactively. A partial `.synrepo/` install is currently treated no differently from an uninitialized repo, which is unsafe and unprofessional.

## What Changes

- Add a smart startup probe so that `synrepo` with no subcommand routes to the correct experience based on runtime readiness.
- Introduce a `runtime-probe` capability that classifies a repo as `uninitialized`, `partial`, or `ready`, and separately reports agent integration readiness.
- Introduce a `dashboard` capability: a dark-theme terminal (TUI) operator surface consuming shared status/watch/setup models, with quick actions for start/stop watch, reconcile now, refresh, and opening setup/repair flows.
- Introduce interactive setup and repair wizards that reuse decomposed `setup` steps. A partial install MUST route to repair, never to greenfield setup.
- Add explicit `synrepo dashboard` subcommand and preserve all existing non-interactive subcommands (no breaking CLI changes).
- Extract a structured status snapshot model from `status.rs` so the CLI renderer and dashboard both consume it; do not duplicate status logic inside the TUI.
- Enforce that the dashboard MUST NOT host the `synrepo mcp` stdio server in-process; it only surfaces MCP readiness and registration.
- Ship one built-in dark palette in v1 with a single `--no-color` escape hatch.

## Capabilities

### New Capabilities
- `runtime-probe`: Classify repo runtime state (uninitialized / partial / ready) and distinguish required runtime readiness from optional agent integration readiness; feeds the smart entrypoint router.
- `dashboard`: Dark-theme TUI operator surface with header, health/activity/next-actions/quick-actions/log panes, and guided setup/repair wizards; consumes shared status and watch models.

### Modified Capabilities
- `bootstrap`: Add the runtime probe and repair-routing contract alongside first-run init; partial state is an explicit classification with a repair path, not a re-init.
- `watch-and-ops`: Expose start/stop/reconcile-now as callable operator actions consumed by the dashboard (contract only, underlying control plane unchanged).

## Impact

- Affected code:
  - `src/bin/cli_support/cli_args.rs`, `src/bin/cli.rs` — add `dashboard`, route bare invocation through probe.
  - `src/bootstrap/` — add `runtime_probe.rs` reusing compat/status signals.
  - `src/bin/cli_support/commands/setup.rs` — decompose into reusable steps (init, write shim, register MCP, apply integration).
  - `src/bin/cli_support/commands/status.rs` — extract a structured snapshot model; CLI renderer becomes a formatter on top.
  - `src/bin/cli_support/commands/watch.rs`, `src/pipeline/watch/{lease,control}.rs` — called by dashboard, not bypassed.
  - New `src/tui/` module tree (`mod`, `app`, `theme`, `probe`, `dashboard`, `wizard`, `actions`, `widgets/`).
- Dependencies: add `ratatui` and `crossterm` (TUI runtime); both widely used, MIT-licensed.
- APIs: no breaking CLI changes. `synrepo` with no subcommand changes from printing help to launching the probe-routed experience; add `--no-color` and an explicit `synrepo dashboard` alias.
- Specs: new `runtime-probe` and `dashboard` specs; deltas to `bootstrap` and `watch-and-ops`.
- Docs: `ROADMAP.md` gains a "Runtime UX and Operator Surface" phase entry.
