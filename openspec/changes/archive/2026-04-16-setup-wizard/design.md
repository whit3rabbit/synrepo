# Design: Setup Wizard for synrepo Client Integration

## Command Surface

### Primary Command

```
synrepo setup
```

Entry point that launches an interactive wizard. No flags required; all options presented via menu.

### Sub-commands (wizard choices)

| Sub-command | Description |
|-------------|-------------|
| `synrepo setup claude` | Setup for Claude Code |
| `synrepo setup opencode` | Setup for OpenCode |
| `synrepo setup codex` | Setup for Codex CLI |
| `synrepo setup cursor` | Setup for Cursor Agent |
| `synrepo setup windsurf` | Setup for Windsurf |
| `synrepo setup copilot` | Setup for GitHub Copilot |
| `synrepo setup gemini` | Setup for Gemini CLI |
| `synrepo setup goose` | Setup for Goose |
| `synrepo setup kiro` | Setup for Kiro CLI |
| `synrepo setup qwen` | Setup for Qwen Code |
| `synrepo setup junie` | Setup for Junie |
| `synrepo setup roo` | Setup for Roo Code |
| `synrepo setup tabnine` | Setup for Tabnine CLI |
| `synrepo setup trae` | Setup for Trae |
| `synrepo setup uninstall` | Remove synrepo integration for a target |
| `synrepo setup status` | Show setup status by target |

### Direct invocation shortcuts

Users can bypass the wizard menu by invoking the target directly:
```
synrepo setup claude --install    # Install Claude Code integration
synrepo setup claude --uninstall  # Remove Claude Code integration
```

## Wizard Flow

### Main Menu

```
╔════════════════════════════════════════╗
║         synrepo Setup Wizard           ║
╠════════════════════════════════════════╣
║  1. Claude Code (claude.ai/code)        ║
║  2. OpenCode (opencode.ai)             ║
║  3. Cursor/Codex                       ║
║  4. Uninstall synrepo                  ║
║  5. View current status                ║
║  q. Quit                               ║
╚════════════════════════════════════════╝

Select an option:
```

### Install Flow (per client)

1. **Check prerequisites**
   - Verify synrepo binary is in PATH
   - If not found, prompt to install from source or binary

2. **Run synrepo init if needed**
   - Check if `.synrepo/` exists in current directory
   - If not, offer to run `synrepo init`
   - Show progress during initialization

3. **MCP Server Injection**
   - Locate user's `.mcp.json` (project-local or home directory)
   - Parse existing JSON without modifying structure
   - Inject synrepo MCP server entry:
     ```json
     {
       "mcpServers": {
         "synrepo": {
           "command": "synrepo",
           "args": ["mcp"],
           "env": {}
         }
       }
     }
     ```
   - Preserve all existing MCP server entries
   - Create backup `.mcp.json.bak` before modification

4. **Client-specific Instructions**
   - Write instruction file to client's expected location
   - For Claude Code: `.claude.md` or update existing
   - For OpenCode: `.opencode.md` or OpenCode-specific config
   - For Cursor/Codex: `.cursorrules` or similar

5. **Gitignore Recommendation**
   - Detect if `.synrepo/` is gitignored
   - If not, suggest adding to `.gitignore`:
     ```
     # synrepo local data
     .synrepo/
     ```

6. **Watch Mode (explicit opt-in)**
   - Never auto-enable
   - Ask: "Would you like to enable watch mode? (y/N)"
   - If yes, show manual command: `synrepo watch`

7. **Final Output**
   - Print summary of what was done
   - If any manual approval needed (e.g., restart IDE), print only that one step

### Uninstall Flow

1. **Confirm uninstall**
   - "This will remove synrepo MCP server from .mcp.json and delete client instruction files."
   - "Continue? (y/N)"

2. **Remove MCP entry**
   - Parse `.mcp.json`
   - Remove only the synrepo entry
   - Restore from backup if removal would corrupt JSON

3. **Remove instruction files**
   - Delete client-specific instruction file created by install

4. **Final Output**
   - "synrepo has been removed from <client>."
   - "To fully remove synrepo, delete the .synrepo/ directory manually."

### Status View

Show current integration status for each supported client:
- Installed/not installed
- MCP server present (yes/no)
- Instruction file present (yes/no)
- Last setup date (if known)

## Technical Implementation

### Dependencies

Add `ratatui` to `Cargo.toml` for TUI framework:
```toml
ratatui = "0.30"
```

ratatui provides:
- Terminal-based UI components (menus, prompts, status views)
- Keyboard navigation
- Cross-platform support via `crossterm`

### File Locations

| Client | MCP Config | Instruction File |
|--------|-----------|-------------------|
| Claude Code | `.mcp.json` (project or `~/.claude/`) | `.claude.md` (project root) |
| OpenCode | `.mcp.json` | `.opencode.md` (project root) |
| Cursor/Codex | `.mcp.json` | `.cursorrules` (project root) |

### Requirement: Client Asset Locations

The setup wizard SHALL model each supported integration as a target-specific asset layout rather than a single generic instruction file.

Each target definition MUST include:
- a stable target key
- a command or skills destination directory
- a file format and extension
- an optional context file location
- whether the target requires its own CLI
- whether synrepo can auto-register MCP for that target

#### Scenario: Installing Claude Code
- **WHEN** installing for Claude Code
- **THEN** synrepo writes skills under `.claude/skills/<skill-name>/SKILL.md`
- **AND** writes or updates `CLAUDE.md`

#### Scenario: Installing OpenCode
- **WHEN** installing for OpenCode
- **THEN** synrepo writes command files under `.opencode/command/`
- **AND** writes or updates `AGENTS.md`

#### Scenario: Installing Codex CLI
- **WHEN** installing for Codex CLI
- **THEN** synrepo writes skills under `.agents/skills/<skill-name>/SKILL.md`
- **AND** writes or updates `AGENTS.md`

#### Scenario: Installing Cursor Agent
- **WHEN** installing for Cursor Agent
- **THEN** synrepo writes skills under `.cursor/skills/<skill-name>/SKILL.md`
- **AND** writes or updates `.cursor/rules/specify-rules.mdc`

#### Scenario: Installing Windsurf
- **WHEN** installing for Windsurf
- **THEN** synrepo writes workflow files under `.windsurf/workflows/`
- **AND** writes or updates `.windsurf/rules/specify-rules.md`

#### Scenario: Installing GitHub Copilot
- **WHEN** installing for GitHub Copilot
- **THEN** synrepo writes agent files under `.github/agents/`
- **AND** writes companion prompt files under `.github/prompts/`
- **AND** writes or updates `.github/copilot-instructions.md`

#### Scenario: Unsupported Automatic MCP Registration
- **WHEN** a target does not have a supported synrepo MCP registration strategy
- **THEN** the wizard still installs skills, commands, and context files
- **AND** prints the one manual next step for MCP registration instead of guessing

### Supported Targets and Asset Locations

| Target | Requires CLI | Commands / Skills Location | Format | Context File | Auto MCP Registration |
|---|---:|---|---|---|---:|
| Claude Code | yes | `.claude/skills/<name>/SKILL.md` | markdown skill | `CLAUDE.md` | yes |
| OpenCode | yes | `.opencode/command/*.md` | markdown | `AGENTS.md` | yes |
| Codex CLI | yes | `.agents/skills/<name>/SKILL.md` | markdown skill | `AGENTS.md` | yes |
| Cursor Agent | no | `.cursor/skills/<name>/SKILL.md` | markdown skill | `.cursor/rules/specify-rules.mdc` | no |
| Windsurf | no | `.windsurf/workflows/*.md` | markdown | `.windsurf/rules/specify-rules.md` | no |
| GitHub Copilot | no* | `.github/agents/*.agent.md` | markdown agent | `.github/copilot-instructions.md` | no |
| Gemini CLI | yes | `.gemini/commands/*.toml` | toml | `GEMINI.md` | no |
| Goose | yes | `.goose/recipes/*.yaml` | yaml | `AGENTS.md` | no |
| Kiro CLI | yes | `.kiro/prompts/*.md` | markdown | `AGENTS.md` | no |
| Qwen Code | yes | `.qwen/commands/*.md` | markdown | `QWEN.md` | no |
| Junie | yes | `.junie/commands/*.md` | markdown | `.junie/AGENTS.md` | no |
| Roo Code | no | `.roo/commands/*.md` | markdown | `.roo/rules/specify-rules.md` | no |
| Tabnine CLI | yes | `.tabnine/agent/commands/*.toml` | toml | `TABNINE.md` | no |
| Trae | no | `.trae/skills/<name>/SKILL.md` | markdown skill | `.trae/rules/project_rules.md` | no |
| Generic | varies | user-provided directory | varies | none | no |

\* Copilot's IDE integration does not require the CLI for install, though workflow dispatch may use the CLI separately.

### `.mcp.json` Injection Strategy

```rust
fn inject_mcp_server(config_path: &Path, server_name: &str, config: &McpServerConfig) -> Result<()> {
    // 1. Read existing file (or create empty structure)
    let existing = if config_path.exists() {
        let content = fs::read_to_string(config_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| Value::Object(Map::new()))
    } else {
        Value::Object(Map::new())
    };

    // 2. Clone and inject
    let mut obj = existing.as_object().cloned().unwrap_or_default();
    let mut servers = obj.get("mcpServers").and_then(|v| v.as_object()).cloned().unwrap_or_default();
    servers.insert(server_name.to_string(), serde_json::to_value(config)?);
    obj.insert("mcpServers".to_string(), Value::Object(servers));

    // 3. Write with backup
    if config_path.exists() {
        let backup_path = config_path.with_extension("json.bak");
        fs::copy(config_path, backup_path)?;
    }
    let output = serde_json::to_string_pretty(&Value::Object(obj))?;
    fs::write(config_path, output)?;

    Ok(())
}
```

### Uninstallation Safety

Before removing from `.mcp.json`:
1. Create backup
2. Parse and verify remaining structure is valid JSON
3. If invalid after removal, restore from backup

### Client Instruction Templates

#### Claude Code (`.claude.md`)

```markdown
# synrepo Integration

synrepo provides code intelligence and graph-based context for this project.

## Available Tools

- `synrepo search <query>` - Search code symbols and concepts
- `synrepo graph query <query>` - Query the code graph
- `synrepo export` - Generate project context cards

## Configuration

Run `synrepo init` to initialize the graph. Use `synrepo watch` to keep it updated.
```

#### OpenCode (`.opencode.md`)

Similar to Claude Code but with OpenCode-specific conventions.

#### Cursor/Codex (`.cursorrules`)

Similar template adapted for Cursor's rules format.

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `.mcp.json` is invalid JSON | Prompt user to fix manually, don't modify |
| No write permission | Error with clear message |
| Binary not in PATH | Prompt to install first |
| Already installed | Offer to reinstall or uninstall |
| Other MCP servers present | Preserve all, never overwrite |

## Future Extensibility

The wizard architecture supports adding new clients:
1. Add client enum variant
2. Implement `ClientSetup` trait with injection logic
3. Add menu option
4. Create instruction template

No changes to core wizard logic needed for new clients.
