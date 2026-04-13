## 1. Watch service runtime

- [x] 1.1 Split the watch runtime into submodules and add a shared service used by foreground and daemon mode
- [x] 1.2 Add per-repo daemon lease and control artifacts under `.synrepo/state/` with stale-owner cleanup
- [x] 1.3 Keep watch-triggered work on `run_reconcile_pass()` with startup reconcile, debounce/coalescing, and `.synrepo/` self-event suppression
- [x] 1.4 Add focused runtime tests for lease acquisition, stale cleanup, control socket requests, lock conflicts, and runtime-only write suppression

## 2. CLI and diagnostics

- [x] 2.1 Add `synrepo watch`, `synrepo watch --daemon`, `synrepo watch status`, and `synrepo watch stop`
- [x] 2.2 Add hidden daemon re-entry via `watch-internal` and detached re-exec spawn logic
- [x] 2.3 Make `synrepo reconcile` delegate to the active watch service and keep unsupported mutating commands fail-fast while watch is active
- [x] 2.4 Extend `synrepo status` with watch summary reporting and add CLI integration coverage for start, status, stop, and delegated reconcile

## 3. Specs, docs, and validation

- [x] 3.1 Update the watch-and-ops spec to define explicit per-repo watch mode, daemon lease versus `writer.lock`, and observable status surfaces
- [x] 3.2 Update `docs/FOUNDATION.md`, `docs/FOUNDATION-SPEC.md`, `ROADMAP.md`, and `AGENTS.md` so they stop treating the MCP server as the daemon and reflect the shipped watch command surface
- [x] 3.3 Verify the slice with focused watch tests, `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `openspec validate daemon-assisted-watch-v1 --strict --type change`
