## 1. Target model

- [x] 1.1 Replace the current `SetupCommand` target list with explicit target variants:
      Claude, OpenCode, Codex, Cursor, Windsurf, Copilot, Gemini, Goose, Kiro,
      Qwen, Junie, Roo, Tabnine, Trae, Uninstall, Status
- [x] 1.2 Add a `SetupTarget` metadata table describing:
      - display name
      - requires_cli
      - command_or_skills_dir
      - extension / format
      - context_file
      - auto_mcp_registration support
- [x] 1.3 Add unit tests that every target has a valid asset layout

## 2. Target-specific asset installation

- [x] 2.1 Replace `instructions.rs` with a more general `assets.rs`
- [x] 2.2 Implement install logic for markdown-skill targets
- [x] 2.3 Implement install logic for markdown-command targets
- [x] 2.4 Implement install logic for TOML-command targets
- [x] 2.5 Implement install logic for YAML-command targets
- [x] 2.6 Implement context-file writers per target
- [x] 2.7 Implement uninstall of all created assets per target

## 3. MCP registration strategy

- [x] 3.1 Separate MCP registration from asset installation
- [x] 3.2 Implement automatic MCP registration only for supported targets
- [x] 3.3 Add `manual_next_step_for_target()` for targets without safe automatic registration
- [x] 3.4 Add tests proving unsupported targets never receive guessed config edits

## 4. First-wave targets

- [x] 4.1 Claude Code
- [x] 4.2 OpenCode
- [x] 4.3 Codex CLI
- [x] 4.4 Cursor Agent
- [x] 4.5 Windsurf
- [x] 4.6 GitHub Copilot

## 5. Second-wave targets

- [x] 5.1 Gemini CLI
- [x] 5.2 Goose
- [x] 5.3 Kiro CLI
- [x] 5.4 Qwen Code
- [x] 5.5 Junie
- [x] 5.6 Roo Code
- [x] 5.7 Tabnine CLI
- [x] 5.8 Trae

## 6. Integration with existing code

- [x] 6.1 Extend existing `AgentTool` enum with new targets
- [x] 6.2 Wire setup wizard to use existing `agent_setup()` function
- [x] 6.3 Preserve backward compatibility with `synrepo agent-setup` command

## 7. Validation

- [x] 7.1 `make check` passes: fmt, clippy, all tests
- [x] 7.2 Verify file sizes: all new `.rs` files under 400 lines
- [x] 7.3 `openspec validate` passes for the change
- [x] 7.4 Manual test: run `synrepo setup` wizard
- [x] 7.5 Manual test: install for Claude Code, verify assets in correct locations
- [x] 7.6 Manual test: verify MCP registration only for supported targets
- [x] 7.7 Manual test: uninstall, verify all assets removed correctly