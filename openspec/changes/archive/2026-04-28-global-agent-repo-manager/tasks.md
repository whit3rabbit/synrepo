## 1. Project Registry Commands

- [x] 1.1 Add `Project` subcommands to `src/bin/cli_support/cli_args/mod.rs`: `add [path]`, `list [--json]`, `inspect [path] [--json]`, and `remove [path]`.
- [x] 1.2 Wire `Command::Project` through `src/bin/cli.rs` and `src/bin/cli_support/commands/mod.rs` without changing existing command behavior.
- [x] 1.3 Implement a new project command module that resolves default paths, canonicalizes targets, and calls existing init/bootstrap readiness paths for `project add`.
- [x] 1.4 Add registry helpers needed by project commands while preserving compatibility for existing `ProjectEntry` and `AgentEntry` TOML.
- [x] 1.5 Implement read-only project health collection for `project list` and `project inspect` using existing runtime probe or store readiness helpers.
- [x] 1.6 Implement non-destructive `project remove`, unregistering only the registry entry and leaving `.synrepo/`, shims, and MCP configs untouched.
- [x] 1.7 Add CLI parse, registry, text output, and JSON output tests for add/list/inspect/remove.

## 2. MCP State Resolution

- [x] 2.1 Split MCP state cache, default-state handling, registry checks, and resolver error formatting out of the over-limit `src/bin/cli_support/commands/mcp.rs`.
- [x] 2.2 Change MCP startup so repo-bound launches still prepare a default state, while global/defaultless launches can start without one when no explicit `--repo` override was supplied.
- [x] 2.3 Make resolver return `Result<Arc<SynrepoState>>` and remove the current fallback-to-default behavior after requested-root preparation failures.
- [x] 2.4 Gate non-default requested roots through `~/.synrepo/projects.toml`, including clear errors for unregistered paths and corrupt registry files.
- [x] 2.5 Canonicalize requested `repo_root` values before cache lookup and registry comparison.
- [x] 2.6 Add tests for default repo resolution, defaultless missing `repo_root`, registered lazy-load, unregistered rejection, corrupt registry rejection, and no fallback on preparation error.

## 3. MCP Tool Routing Audit

- [x] 3.1 Add `repo_root` parameters to repository-scoped MCP parameter structs that lack them.
- [x] 3.2 Update graph primitive, search, where-to-edit, impact/risk, entrypoint, module/public API, notes, findings, recent activity, and workflow alias handlers to use the resolved target state.
- [x] 3.3 Update workflow metrics recording so it is associated with the resolved repository or skipped when state resolution fails.
- [x] 3.4 Ensure edit-capable tools keep the existing `--allow-edits` gate while using the new resolver.
- [x] 3.5 Update MCP tool descriptions where needed so repo-root behavior is visible to agents.
- [x] 3.6 Add focused MCP handler tests that prove `repo_root` routes to the requested registered repository for at least one card/search tool and one graph primitive.

## 4. Setup And Global Agent Integration

- [x] 4.1 Ensure successful `synrepo setup <tool>` and `synrepo setup <tool> --global` register the current project exactly once.
- [x] 4.2 Confirm supported global MCP writers emit `synrepo mcp` without `--repo .`, and project-scoped writers continue to emit `synrepo mcp --repo .`.
- [x] 4.3 Fix or explicitly reject global setup for targets whose writer currently ignores the `global` flag, such as Codex and OpenCode.
- [x] 4.4 Update setup summaries and failure paths so mixed multi-client global setup reports supported and unsupported targets clearly.
- [x] 4.5 Add setup writer and orchestration tests for global registration, unsupported global targets, and project registration after successful setup.

## 5. Agent Doctrine And Generated Guidance

- [x] 5.1 Update the canonical doctrine block to explain global MCP `repo_root` behavior and the `synrepo project add <path>` remedy.
- [x] 5.2 Update target-specific shim text where it references MCP install paths or examples, keeping doctrine shared through the existing macro path.
- [x] 5.3 Update `skill/SKILL.md` or generated-skill fixtures if tests require byte-identical doctrine or tool list changes.
- [x] 5.4 Add or update tests that enforce doctrine consistency across shims and confirm global guidance appears once.

## 6. Validation

- [x] 6.1 Run `openspec status --change global-agent-repo-manager --json` and confirm `isComplete` remains false until implementation tasks are checked off.
  - Observed: `isComplete` was already `true` before implementation tasks were checked off, so current OpenSpec status appears to validate artifact completeness rather than task checkbox state.
- [x] 6.2 Run focused Rust tests for registry/project commands, setup/global writers, and MCP resolver/tool routing.
- [x] 6.3 Run `cargo fmt`.
- [x] 6.4 Run `make ci-lint`.
- [x] 6.5 Run `make ci-test` or a documented narrower substitute if local time is constrained.
- [x] 6.6 Run `openspec status --change global-agent-repo-manager --json` after checking tasks off and confirm the change is complete before archive.
