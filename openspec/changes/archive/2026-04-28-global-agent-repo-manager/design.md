## Context

Synrepo already has several pieces of a global integration model, but they are not yet composed into one contract.

- `~/.synrepo/projects.toml` exists, but `src/registry/mod.rs` describes it as an install/uninstall registry. It records project paths, gitignore mutations, and per-agent install records, not a user-facing project manager.
- `synrepo setup <tool> --global` is already parsed by the CLI and several MCP writers can emit a global entry that launches `synrepo mcp`. Codex and OpenCode currently ignore the `global` parameter, so a global request can still write project-local config for those targets.
- `SynrepoServer` already stores states by repo root, but `resolve_state` silently falls back to the default repo if `prepare_state` fails for a requested root.
- Many MCP tools still call `resolve_state(None)` even when their parameter type has or should have a `repo_root` field.
- `synrepo mcp` currently prepares the default repo before serving, which means a global agent config launched outside an initialized repository can fail before a tool call supplies `repo_root`.
- `src/bin/cli_support/commands/mcp.rs` is 721 lines, above the repository cap. Adding more routing logic there directly would make an existing structural problem worse.

## Goals / Non-Goals

**Goals:**

- Add a `synrepo project` command group for registering, listing, inspecting, and unregistering repositories.
- Treat the user registry as the authorization and discovery source for cross-repo MCP routing.
- Make `synrepo mcp` work in both repo-bound and global/defaultless contexts without requiring a separate `mcp --global` mode.
- Preserve existing per-repo MCP behavior: `synrepo mcp --repo .` may still use the default root when tools omit `repo_root`.
- Return explicit MCP errors for missing `repo_root`, unregistered paths, uninitialized repos, compatibility failures, and other state-preparation failures.
- Update generated agent guidance so global integrations pass an absolute workspace path to repo-addressable tools.

**Non-Goals:**

- No global multi-repo watch daemon in this change.
- No deletion of a repository's `.synrepo/` directory from `project remove`.
- No relocation of graph, overlay, lexical, or explain stores out of each repository.
- No new cross-session agent memory or task tracking semantics.

## Decisions

### 1. Use a hybrid MCP server, not a required `--global` flag

`synrepo mcp` should start with an optional default state. If the command is launched inside or with an initialized repo, that repo is the default and current repo-bound behavior remains. If the command is launched from a non-repo context by a global agent config, the server can still start without a default state and require `repo_root` on repo-addressable calls.

Alternative considered: add `synrepo mcp --global` as a separate mode. This would make the command line explicit, but it creates another path agents must learn and conflicts with the current global setup writers that already emit `synrepo mcp`.

### 2. Make state resolution fallible and registry-gated

Replace `resolve_state(...) -> Arc<SynrepoState>` with a fallible resolver. It should canonicalize requested paths, check the in-memory cache, validate the path against `~/.synrepo/projects.toml` unless it is the server's default root, and only then call `prepare_state`.

Errors must propagate to the tool response. Falling back to the default root after a failed requested root is incorrect in a global model because it can return truthful data for the wrong repository.

### 3. Split MCP routing before adding behavior

Move state resolution and default-state startup into a smaller helper module, for example `commands/mcp/state.rs` or `commands/mcp_state.rs`, before expanding behavior. Tool registration may remain where existing tests expect it, but the cache, registry checks, defaultless startup, and error formatting should leave the over-limit `mcp.rs` file.

Alternative considered: patch `mcp.rs` in place. That is faster, but it violates the repository's file-size rule and increases risk in one of the hottest files in the codebase.

### 4. Compute project health at list time

Do not persist health in `projects.toml`. Health is derived from the filesystem and store compatibility, so persisting it would go stale. `synrepo project list` and `synrepo project inspect` should load registry entries and run a read-only probe for each path.

`auto_sync` or daemon subscription flags should wait for the later global daemon change. This change can preserve a future-compatible registry shape with serde defaults, but it should not promise daemon semantics before a daemon exists.

### 5. Keep project removal non-destructive

`synrepo project remove [path]` unregisters the project from `projects.toml`. It does not delete `.synrepo/`, strip shims, or edit agent configs. Existing `synrepo remove` remains the destructive uninstall workflow with its own prompts and dry-run behavior.

### 6. Extend existing setup instead of adding a parallel install command

The existing `synrepo setup <tool> --global` flag is the right entry point for global MCP registration. The implementation should make that path reliable, register the current project, and clearly report unsupported global targets rather than adding `synrepo install <agent>` as a second setup vocabulary.

## Risks / Trade-offs

- Defaultless MCP startup could expose old tools that forgot to pass `repo_root` -> mitigate by auditing every repo-addressable tool, adding `repo_root` parameters where missing, and returning a clear "repo_root is required" error when no default exists.
- Registry gating could reject legitimate temporary repos -> mitigate by telling agents and users to run `synrepo project add <path>` and keeping per-repo `--repo` invocation available for ad hoc use.
- Canonical path handling can be confusing around symlinks and deleted paths -> mitigate by using the existing registry canonicalization behavior and showing both stored path and health in list output.
- Global setup edits user-level config files -> mitigate by reusing existing JSON/TOML merge helpers, preserving unrelated entries, and making unsupported global targets explicit.
- MCP telemetry currently records workflow calls against the default state -> mitigate by recording metrics against the resolved target state or skipping per-repo metrics when resolution fails.

## Migration Plan

1. Add project-manager CLI behavior on top of the existing registry with serde defaults for any new optional fields.
2. Change MCP startup to allow an optional default state while preserving explicit `--repo` failure behavior.
3. Make MCP state resolution fallible, registry-gated, and non-fallback.
4. Audit MCP tools and parameter structs for `repo_root` consistency.
5. Update setup/global registration and generated doctrine.
6. Validate with focused CLI parse tests, registry tests, MCP resolver tests, and setup writer tests before broader CI.

Rollback is straightforward: per-repo `synrepo mcp --repo .` and existing project-local agent configs remain valid. Removing global entries from agent configs and unregistering projects returns users to the current model without deleting repository state.

## Open Questions

- Should `synrepo project add` always initialize an uninitialized repository, or should it support a strict `--no-init` registration-only mode for advanced users? The recommended first implementation should initialize by default and skip `--no-init` unless a real workflow demands it.
