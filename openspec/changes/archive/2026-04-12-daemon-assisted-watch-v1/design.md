## Context

synrepo already has the right correctness model for watch mode: the watcher is a trigger and coalescing layer, and reconcile is the deterministic backstop that reruns the real producer path. What was missing was the operational shell around that behavior: an explicit command surface, a per-repo watch owner, a small control plane, and docs that stop conflating the MCP server with the daemon.

This change keeps the current architectural boundary intact. The stdio MCP server remains an agent-facing read surface. The watch service is a separate optional local runtime that owns one repo's watch lifecycle only when the user starts it.

## Goals / Non-Goals

**Goals:**

- Ship explicit per-repo watch mode in foreground and daemon-assisted forms.
- Keep one shared watch service implementation for both modes.
- Record long-lived watch ownership and telemetry separately from operation-scoped write locking.
- Make watch behavior observable through status, stop, and delegated reconcile paths.
- Update the durable watch contract and runtime docs to match the implementation.

**Non-Goals:**

- Make watch mode the default operating model.
- Fold watch lifecycle into the stdio MCP server.
- Add Git/HEAD-triggered refresh hooks in this slice.
- Add global registry, auto-discovery, launchd integration, or start-at-login behavior.
- Invent a partial-update path separate from `run_reconcile_pass()`.

## Decisions

1. Use explicit per-project opt-in watch mode.
   Users start watch with `synrepo watch` or `synrepo watch --daemon`. No repo is watched automatically.
   Alternative considered: global always-on watch. Rejected because it hides ownership and creates background state the user did not ask for.

2. Keep foreground and daemon mode on one service implementation.
   The service performs startup reconcile, then enters the debounced watch loop, regardless of whether it runs attached or detached.
   Alternative considered: separate foreground and daemon codepaths. Rejected because it would duplicate correctness logic and invite drift.

3. Separate long-lived watch ownership from operation-scoped write locking.
   `.synrepo/state/watch-daemon.json` and `.synrepo/state/watch.sock` represent watch ownership and control. `.synrepo/state/writer.lock` still guards actual writes only.
   Alternative considered: make `writer.lock` the daemon lease. Rejected because it would blur "watch owner exists" with "a write is happening now."

4. Keep the daemon as an orchestration layer over `run_reconcile_pass()`.
   Filesystem events trigger bounded reconcile cycles. `reconcile` forwards `reconcile_now` when watch is active. Other mutating commands either become daemon-aware explicitly or fail fast.
   Alternative considered: ad hoc daemon-side partial mutations. Rejected because it would create a second truth path.

5. Make operational state visible at the repo level.
   `synrepo watch status` and `synrepo status` expose watch ownership, last reconcile outcome, and stale-artifact state. `synrepo watch stop` provides explicit shutdown and cleanup.
   Alternative considered: implicit shutdown via PID/liveness checks only. Rejected because operators should not infer state from silent artifacts.

## Risks / Trade-offs

- Detached daemon startup adds local process and socket lifecycle complexity, mitigated by a per-repo lease file, stale cleanup, and explicit stop/status commands.
- Some mutating commands now fail while watch is active unless they are made daemon-aware, mitigated by clear error messages and keeping read-only commands local.
- Watching `.synrepo/` would create self-triggered churn, mitigated by suppressing runtime-path events and running the startup reconcile before the watcher is attached.
- Unix domain sockets keep this slice small and reliable on macOS/Linux, but foreground watch remains the fallback mode where daemon control transport is unavailable.

## Migration Plan

1. Keep standalone CLI behavior as the default. Existing users do not need to change anything.
2. Allow users to opt a repo into foreground or daemon-assisted watch mode explicitly.
3. Route `reconcile` through the active watch service when present; fail fast for other mutating commands until they gain explicit daemon-aware behavior.
4. Surface watch state in status output so stale artifacts and ownership are diagnosable before adding deeper background automation.

## Open Questions

- Whether future daemon-aware mutating commands should forward via the same control socket or a richer job protocol once more than reconcile needs delegation.
