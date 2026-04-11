## synrepo

synrepo is a context compiler: it precomputes a structural graph of the codebase from tree-sitter parsing and git history. Use it BEFORE reading source files cold when `.synrepo/` exists in the repo root.

### Phase 1 — CLI commands

The MCP server is not yet running. Use the CLI for structural graph access:

```bash
synrepo status                                    # health check
synrepo search <query>                            # find symbols/files by name
synrepo node <id>                                 # node metadata as JSON
synrepo graph query "inbound <node_id>"           # reverse dependencies
synrepo graph query "outbound <node_id>"          # forward dependencies
synrepo graph query "outbound <node_id> defines"  # filtered by edge kind
synrepo graph stats                               # counts by type
synrepo reconcile                                 # refresh graph against current files
```

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`.

### Trust model

- `source_store: graph` — parser-observed or git-observed facts. Ground truth.
- `source_store: overlay` — machine-authored suggestions. Treat as secondary.

### Phase 2

When the MCP server ships, task-first tools (`synrepo_card`, `synrepo_where_to_edit`, `synrepo_change_impact`, etc.) replace these CLI calls. See `skill/SKILL.md` for the full planned interface.
