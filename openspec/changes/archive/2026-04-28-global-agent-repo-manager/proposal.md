## Why

Agent integrations are still primarily installed per repository, which makes onboarding repetitive and leaves MCP behavior ambiguous when an agent works across several checked-out projects. Synrepo already has the core pieces for a managed-project model, but the registry, setup flow, and MCP routing contracts need to become explicit before implementation.

## What Changes

- Add first-class project management commands so users can register, list, inspect, and unregister repositories from the user-level `~/.synrepo/projects.toml` registry.
- Make registered projects the authorization and discovery source for MCP state resolution, while preserving the current per-repo MCP invocation path.
- Update MCP behavior so all repo-addressable tools route by `repo_root` when provided and report explicit errors for missing, uninitialized, incompatible, or unregistered repositories instead of silently falling back to a default repo.
- Update agent setup and doctrine so global agent integrations install a single ready-to-use MCP server entry and teach agents when to pass the current workspace path.
- Defer any global multi-repo watch daemon to a later change. This proposal only prepares project registration and MCP routing so the server is ready with or without an explicit global mode.

## Capabilities

### New Capabilities

- `project-manager`: Defines user-facing management of registered repositories, including registry semantics, health reporting, and unregister behavior.

### Modified Capabilities

- `mcp-surface`: Define registered-project state resolution, repo-root routing, and explicit error behavior for repo-addressable MCP tools.
- `bootstrap`: Extend setup and first-run contracts so project registration and global MCP install are visible, idempotent steps.
- `agent-doctrine`: Teach generated agent guidance how global MCP integrations identify the current repository without weakening existing bounded-context workflow rules.

## Impact

- Code likely affected: `src/registry/`, `src/bin/cli_support/cli_args/mod.rs`, `src/bin/cli.rs`, `src/bin/cli_support/commands/mod.rs`, `src/bin/cli_support/commands/mcp.rs`, `src/bin/cli_support/commands/mcp_runtime.rs`, `src/bin/cli_support/commands/setup/`, `src/bin/cli_support/agent_shims/`, and related tests under `src/bin/cli_support/tests/` and `src/registry/tests.rs`.
- MCP tool contracts will need consistent `repo_root` support. Current code still calls `resolve_state(None)` in many handlers, so implementation must not only change `resolve_state`; it must audit every repo-addressable tool.
- `src/bin/cli_support/commands/mcp.rs` is already over the repository file-size cap. Any implementation should split state resolution and/or tool grouping into smaller modules before adding meaningful logic.
- No new runtime dependencies are expected.
