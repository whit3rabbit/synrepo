## Context

Phase 1 is complete. The graph is built, rename detection is wired, reconcile is running.
`synrepo status` is shipped. The next gap is agent CLI integration: agents that load
`skill/SKILL.md` see Phase 2 MCP tool descriptions with no context about what Phase 1
offers today; other agent CLIs have no integration path at all.

The RTK design review surfaced a useful principle: integration shims should be dumb. The
shim checks for `.synrepo/`, documents CLI commands, and references the binary. All real
logic stays in the binary. The shim is a thin delegator, not a reimplementation.

## Design Decisions

### skill/SKILL.md current-phase section

**Decision:** Add a clearly marked "Current phase" section at the very top of the skill,
before all tool descriptions, that:
- states Phase 1 is complete and Phase 2 (MCP) is not yet shipped
- lists available Phase 1 CLI commands with brief descriptions
- tells the agent to use the CLI fallback section rather than attempting MCP calls

This section is removed (or replaced with a Phase 2 section) when the MCP server ships.
The skill's existing tool descriptions remain intact as the authoritative Phase 2 contract.

**Why:** Without this, agents that load the skill will attempt MCP tool calls that return
errors, then fall back to cold file reads. The Phase 1 CLI is functional; agents just need
to know it exists.

### synrepo agent-setup — explicit command, no auto-generation during init

**Decision:** `synrepo agent-setup <tool>` is an explicit command the user runs once, not
something `synrepo init` triggers automatically.

**Why:** Automatic generation during init would write files into directories the user may
not want modified (`.claude/`, `.cursor/`). The explicit command model follows the RTK thin
delegator principle: the user decides when to wire the integration, not the init flow.

### Output file placement and no-clobber behavior

Each shim target writes to a fixed output path relative to the repo root:

| Target  | Output path                      | Instruction printed                          |
|---------|----------------------------------|----------------------------------------------|
| claude  | `.claude/synrepo-context.md`     | Include with `@.claude/synrepo-context.md`   |
| cursor  | `.cursor/synrepo.mdc`            | Add `.cursor/synrepo.mdc` to your rules      |
| copilot | `synrepo-copilot-instructions.md`| Paste into `.github/copilot-instructions.md` |
| generic | `synrepo-agents.md`              | Paste into `AGENTS.md`                       |

**No-clobber:** if the output file already exists, the command prints a warning and exits
without overwriting. The user can pass `--force` to overwrite.

**Why:** These files may contain user customizations. Silently clobbering them would be a
worse experience than the command doing nothing.

### Shim content: what each file contains

Every shim contains the same structural sections, adapted to the target format:

1. **What synrepo is** — one-sentence positioning ("a context compiler for AI coding agents")
2. **When to use it** — check for `.synrepo/` directory first
3. **Current phase (Phase 1)** — CLI commands available today
4. **Phase 2 note** — MCP tools are planned; skill/SKILL.md has the full description
5. **Commands quick reference** — `status`, `reconcile`, `search`, `graph query`, `graph stats`, `node`

The shim does NOT contain:
- long-form explanations (those live in skill/SKILL.md)
- any tool descriptions that describe MCP tools
- any generated content that would need to be kept in sync with the binary's output format

**Why:** Thin shims age well. Fat shims drift. The source of truth for tool behavior is
the binary and skill/SKILL.md, not the per-tool integration file.

### Implementation: shim content lives in the binary

Shim content is embedded as static strings in the Rust binary (or generated from a small
set of string templates). It is not read from external template files at runtime.

**Why:** Simpler to distribute. No risk of missing template files. Shim content is small
and versioned with the binary.

### No test for shim content correctness

The shim content is static/generated text. A doc-drift test (similar to `docs_drift.rs`)
can verify that the CLI commands mentioned in shims still exist, but that is optional
follow-on work. The core contract — that the command runs, creates the file, and prints
the instruction — is verifiable with a simple integration test.

## Architecture

```
src/bin/cli.rs
  └── Command::AgentSetup { tool: AgentTool }
        └── cli_support/commands.rs: agent_setup(repo_root, tool)
              └── cli_support/agent_shims.rs: shim_content(tool) -> &'static str
                                              output_path(repo_root, tool) -> PathBuf
                                              include_instruction(tool) -> &'static str
```

`agent_shims.rs` is a new module containing:
- `AgentTool` enum (Claude, Cursor, Copilot, Generic)
- `shim_content(tool)` — returns the static shim text
- `output_path(repo_root, tool)` — returns the target file path
- `include_instruction(tool)` — returns the one-line user instruction to print

`commands.rs::agent_setup` handles:
- create parent directories if needed
- check for existing file and respect no-clobber rule
- write the shim
- print the include instruction

## Sequencing

This change does not depend on any unshipped work. It can be implemented directly against
the Phase 1 CLI surface.

The `synrepo agent-setup` command should produce output that references `synrepo status`
as a health check, so `status` should ship first. It already has.
