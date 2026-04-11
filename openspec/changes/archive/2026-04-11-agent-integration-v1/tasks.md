## Tasks

### Task 1: Update skill/SKILL.md with current-phase section

- [x] Add a "Current phase (Phase 1 — structural graph, no MCP yet)" section at the top
      of `skill/SKILL.md`, before the "When to use synrepo" section
- [x] List available CLI commands: `status`, `reconcile`, `search`, `graph query`,
      `graph stats`, `node`
- [x] Expand the existing "Falling back when the MCP server isn't available" section to
      include all current commands with brief descriptions

Acceptance: `skill/SKILL.md` first section tells agents the MCP server is not yet running
and lists the CLI commands they should use instead.

---

### Task 2: Add agent_shims module

- [x] Create `src/bin/cli_support/agent_shims.rs`
- [x] Define `AgentTool` enum: `Claude`, `Cursor`, `Copilot`, `Generic`
- [x] Implement `shim_content(tool: AgentTool) -> &'static str` — static shim text per tool
- [x] Implement `output_path(repo_root: &Path, tool: AgentTool) -> PathBuf` — target file path
- [x] Implement `include_instruction(tool: AgentTool) -> &'static str` — one-line user instruction

Shim content per tool:
- `claude`: `.claude/synrepo-context.md` — markdown with synrepo Phase 1 CLI reference
- `cursor`: `.cursor/synrepo.mdc` — Cursor rule fragment
- `copilot`: `synrepo-copilot-instructions.md` — paste-ready GitHub Copilot instructions
- `generic`: `synrepo-agents.md` — paste-ready generic AGENTS.md fragment

Acceptance: module compiles; shim content compiles as static str; paths are correct.

---

### Task 3: Add agent_setup command

- [x] Add `AgentSetup { tool: AgentToolArg }` variant to `Command` enum in `cli.rs`
- [x] Add `AgentToolArg` clap value enum mapping to `AgentTool`
- [x] Add `--force` flag to `AgentSetup` command
- [x] Implement `agent_setup(repo_root, tool, force)` in `commands.rs`:
  - compute output path
  - if file exists and `!force`, print warning and exit `Ok(())`
  - create parent directories
  - write shim content
  - print include instruction

Acceptance: `cargo run -- agent-setup claude` creates `.claude/synrepo-context.md` and
prints the include instruction; re-running without `--force` prints the warning; `--force`
overwrites; `cargo clippy -- -D warnings` passes; `cargo test` passes.

---

### Task 4: Verify and close

- [x] Run `cargo test` — all tests pass
- [x] Run `cargo clippy -- -D warnings` — no warnings
- [x] Run `cargo run -- agent-setup claude` and verify output file content is correct
- [x] Run `cargo run -- agent-setup generic` and verify AGENTS.md fragment is correct
- [x] Confirm `skill/SKILL.md` current-phase section is accurate for the current CLI surface
