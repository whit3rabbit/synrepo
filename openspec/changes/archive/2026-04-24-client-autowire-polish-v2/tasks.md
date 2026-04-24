## 1. Detection And Report Model

- [x] 1.1 Define per-client outcome statuses for detected, written, registered, current, skipped, unsupported, stale, and failed.
- [x] 1.2 Add project/global scope and target path fields to setup reporting.
- [x] 1.3 Add shim freshness classification separate from MCP registration.

## 2. Setup Integration

- [x] 2.1 Update setup and agent-setup output to print a detected-client summary.
- [x] 2.2 Preserve single positional target behavior and `--only` / `--skip` behavior.
- [x] 2.3 Report stale shims with `--regen` guidance without overwriting them silently.

## 3. Doctrine Consistency

- [x] 3.1 Ensure generated shim freshness compares against the canonical doctrine block.
- [x] 3.2 Update first-run or setup pointer text only where it consumes the new report data.
- [x] 3.3 Add tests for current shim, stale shim, missing MCP registration, skipped target, and failed target output.

## 4. Verification

- [x] 4.1 Run focused setup and agent-setup tests.
- [x] 4.2 Run `cargo test` for agent shim and bootstrap reporting surfaces.
- [x] 4.3 Run `openspec validate client-autowire-polish-v2`.
- [x] 4.4 Run `openspec status --change client-autowire-polish-v2 --json` and confirm `isComplete: true`.
