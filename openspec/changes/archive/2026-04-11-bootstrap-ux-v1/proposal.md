## Why

The repo already has a working `synrepo init` skeleton, but the bootstrap UX is still too thin for a real first-run contract. Mode selection is only an explicit flag, health states are not defined, re-running init hard-fails without guidance, and the first-run output does not yet tell the user enough about what happened or what to do next.

## What Changes

- Define the first real bootstrap UX for `synrepo init`, including mode-selection behavior, explicit health states, and mandatory first-run output.
- Lock re-entry behavior for already-initialized repositories and partially usable `.synrepo/` state.
- Define when bootstrap recommends or selects auto versus curated mode based on repository signals and explicit user input.
- Define the minimum project-health summary and next-step guidance a successful init must emit.
- Add bootstrap tests and validation for mode selection, init refusal or re-entry, and first-run output behavior.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `bootstrap`: sharpen init, mode selection, health states, first-run summary, and re-entry behavior into implementable UX rules

## Impact

- Affects the CLI surface (`src/bin/cli.rs`, `src/bin/cli_support/`) and config/bootstrap behavior in `src/config.rs`
- May add bootstrap helpers for repository inspection and health reporting
- Adds tests around init behavior and CLI output
- Does not introduce cards, MCP tools, or overlay generation
