## Why

Milestone 4 needs a cheap, truthful way to detect and repair stale synrepo surfaces after structural facts, rationale links, and generated views drift apart. The repository already has runtime diagnostics, storage maintenance, the five-tool MCP read surface, and reconcile backstops, but it does not yet expose a selective repair loop that lets users fix only what drifted instead of rerunning broad setup flows or inspecting stale state manually.

## What Changes

- Define the first concrete repair workflow around `synrepo check` and `synrepo sync`.
- Define machine-readable drift classes and repair actions for stale exports, stale runtime views, broken declared links, stale rationale, stale overlay entries, and trust conflicts.
- Define how repair reports distinguish actionable, report-only, unsupported, and blocked findings so absent future surfaces are visible without being treated as failures.
- Define resolution logging for repair runs so local CLI and CI flows can audit what was detected, what was repaired, and what still needs human attention.
- Keep repair scoped to targeted surfaces and existing producer paths, without collapsing graph and overlay into one trust bucket.

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `repair-loop`: Expand the repair-loop contract from high-level goals into concrete `check` and `sync` behavior, drift finding classes, targeted repair rules, unsupported-surface handling, and resolution logging.

## Impact

- `src/bin/cli.rs` and `src/bin/cli_support/commands.rs` will gain `check` and `sync` command handling.
- `src/pipeline/diagnostics.rs`, `src/pipeline/maintenance.rs`, and `src/pipeline/watch.rs` provide the current health and repair primitives this change will compose instead of replacing.
- A new repair-planning/execution module under `src/pipeline/` will likely own drift findings, repair plans, and resolution-log writing.
- `.synrepo/state/` will gain a repair audit artifact (for example an append-only resolution log) in addition to the existing reconcile state.
- No new external dependencies are expected.
