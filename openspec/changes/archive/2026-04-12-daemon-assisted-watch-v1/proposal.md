## Why

The watch-and-reconcile foundation exists, but synrepo still needs an explicit optional runtime mode that keeps one initialized repository fresh without turning background behavior into a mystery. The current direction already points there: watcher churn should coalesce into bounded reconcile work, reconcile remains the correctness backstop, and daemon-assisted operation is allowed but not required.

This change lands that next slice as an operational feature, not a new product personality. Standalone CLI behavior stays the baseline. A per-repo watch service becomes an explicit opt-in accelerator for reducing drift window and keeping the lexical and structural runtime warm.

## What Changes

- Add explicit per-project watch commands: `synrepo watch`, `synrepo watch --daemon`, `synrepo watch status`, and `synrepo watch stop`.
- Add a shared watch service for foreground and daemon mode, with startup reconcile, debounce/coalescing, and `.synrepo/` self-event suppression over the existing `run_reconcile_pass()` path.
- Add a per-repo daemon lease and control plane under `.synrepo/state/` using `watch-daemon.json` and `watch.sock`.
- Keep `writer.lock` operation-scoped while the watch daemon lease records long-lived watch ownership.
- Make `synrepo reconcile` daemon-aware and keep other mutating commands fail-fast unless they are explicitly watch-aware in this slice.
- Update the watch-and-ops contract and runtime docs so they stop treating the stdio MCP server as the daemon or single writer.

## Capabilities

### New Capabilities

- `watch-and-ops`: explicit per-repo foreground and daemon-assisted watch mode with observable ownership and control

### Modified Capabilities

- `watch-and-ops`: clarify daemon lease versus `writer.lock`, reconcile delegation, and operator-facing status surfaces

## Impact

- Affects the CLI command surface, watch runtime code, runtime state layout under `.synrepo/state/`, and operational diagnostics
- Keeps daemon behavior separate from the stdio MCP server and does not change `Config.mode`
- Adds tests for daemon lease acquisition, stale cleanup, control socket requests, self-event suppression, delegated reconcile, and status output
- Does not add Git/HEAD-triggered refresh hooks, auto-start at login, or global project discovery
