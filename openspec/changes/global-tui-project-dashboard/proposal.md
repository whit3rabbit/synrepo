## Why

The dashboard was intentionally built as a single-repository operator surface, but global MCP setup now depends on a user-level project registry that can serve many repositories from one binary. The TUI needs to expose that same project model so users can quickly select, monitor, and act on registered projects without ambiguity.

## What Changes

- Extend the existing project registry from install bookkeeping into a user-facing project selector by adding stable project IDs, display names, and last-opened ordering.
- Add project CLI conveniences for selecting and renaming registered projects without changing repository paths or `.synrepo/` storage.
- Add a global TUI shell that owns registered projects, tracks one active project, and keeps repo-specific dashboard runtime state scoped per project.
- Add a fast project picker and project-scoped watch visibility to the dashboard.
- Make TUI actions project-explicit so reconcile, sync, materialize, watch, explain, and docs actions cannot silently run against the wrong repository after a switch.
- Improve dashboard safety and accessibility: confirm heavyweight actions, preserve essential footer hints during toasts, support reduced-motion/ASCII rendering, and replace fixed Live pagination with viewport-aware movement.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `project-manager`: Add stable project identity, display aliases, recent ordering, rename/use resolution, and TUI-safe detach semantics on top of the existing registry.
- `dashboard`: Expand the interactive dashboard from a single-repository surface into a global project shell with an active project, project picker, project-scoped state, explicit action context, and accessibility/safety behavior.

## Impact

- `src/registry/`: backward-compatible registry schema and project lookup helpers.
- `src/bin/cli_support/`: project subcommands and bare-entry project resolution.
- `src/tui/`: global app shell, project picker, header/footer/help/accessibility widgets, project-scoped action dispatch.
- `openspec/specs/project-manager/spec.md` and `openspec/specs/dashboard/spec.md`: enduring behavior updated through delta specs.
- Tests: registry compatibility, project CLI resolution, TUI switching/action scope, footer/modal/page/accessibility widget behavior.
