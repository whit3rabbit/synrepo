## 1. Setup wizard core command

- [ ] 1.1 Add `ratatui` dependency to `Cargo.toml`
- [ ] 1.2 Implement `SetupCommand` enum with variants: Claude, Opencode, Codex, Uninstall, Status
- [ ] 1.3 Create `src/bin/cli_support/commands/setup.rs` module
- [ ] 1.4 Implement wizard menu loop with interactive selection
- [ ] 1.5 Add unit tests for command parsing

## 2. MCP configuration injection

- [ ] 2.1 Create `src/surface/setup/mcp_inject.rs` module
- [ ] 2.2 Implement `locate_mcp_config() -> Option<PathBuf>` to find .mcp.json
- [ ] 2.3 Implement `inject_synrepo_server(config_path, server_config) -> Result<()>` for injection
- [ ] 2.4 Implement `remove_synrepo_server(config_path) -> Result<()>` for uninstall
- [ ] 2.5 Implement `backup_config(config_path) -> Result<PathBuf>` for safety
- [ ] 2.6 Add unit tests for injection/removal with mocked filesystem

## 3. Client-specific instruction files

- [ ] 3.1 Create `src/surface/setup/instructions.rs` module
- [ ] 3.2 Define instruction template for Claude Code (`.claude.md`)
- [ ] 3.3 Define instruction template for OpenCode (`.opencode.md`)
- [ ] 3.4 Define instruction template for Cursor/Codex (`.cursorrules`)
- [ ] 3.5 Implement `write_client_instructions(client, project_root) -> Result<PathBuf>`
- [ ] 3.6 Implement `remove_client_instructions(client, project_root) -> Result<()>`
- [ ] 3.7 Add unit tests for instruction file writing

## 4. Gitignore handling

- [ ] 4.1 Create `src/surface/setup/gitignore.rs` module
- [ ] 4.2 Implement `check_gitignore_synced(project_root) -> bool`
- [ ] 4.3 Implement `suggest_gitignore_entry(project_root) -> String` returning the entry
- [ ] 4.4 Add unit tests for gitignore checking

## 5. Init orchestration

- [ ] 5.1 Implement `ensure_synrepo_initialized(project_root) -> Result<bool>` checking .synrepo/ exists
- [ ] 5.2 If not initialized, prompt user and run init via subprocess
- [ ] 5.3 Add integration test for init orchestration flow

## 6. Wizard flow implementation

- [ ] 6.1 Implement main menu display and input handling
- [ ] 6.2 Implement install flow per client (steps in order: init -> MCP inject -> instructions -> gitignore -> watch prompt)
- [ ] 6.3 Implement uninstall flow (confirm -> remove MCP -> remove instructions)
- [ ] 6.4 Implement status view showing current integration per client
- [ ] 6.5 Add one-next-step printer for manual approval requirements
- [ ] 6.6 Add integration tests for wizard flows

## 7. OpenCode target

- [ ] 7.1 Add OpenCode as explicit client variant (not generic)
- [ ] 7.2 Research OpenCode-specific instruction file conventions
- [ ] 7.3 Create OpenCode instruction template
- [ ] 7.4 Add to wizard menu

## 8. Validation

- [ ] 8.1 `make check` passes: fmt, clippy, all tests
- [ ] 8.2 Verify file sizes: all new `.rs` files under 400 lines
- [ ] 8.3 `openspec validate` passes for the change
- [ ] 8.4 Manual test: run `synrepo setup` wizard
- [ ] 8.5 Manual test: install for Claude Code, verify .mcp.json updated
- [ ] 8.6 Manual test: uninstall, verify .mcp.json restored correctly
