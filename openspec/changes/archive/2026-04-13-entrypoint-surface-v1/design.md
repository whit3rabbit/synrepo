## Context

The structural pipeline already produces `FileNode`, `SymbolNode`, and `Defines` / `Calls` / `Imports` edges. `ModuleCard` is a struct placeholder with no compiler method. `EntryPointCard` has no struct, no compiler, and no detection logic. The five-tool MCP surface does not expose `synrepo_entrypoints`.

Entry-point detection belongs at card-compile time, not in a new pipeline stage: the required signals (symbol names, file paths, `SymbolKind`) are already present in the graph, and adding a pipeline stage would require persisting derived facts that are cheap to re-derive on demand.

## Goals / Non-Goals

**Goals:**
- Wire `GraphCardCompiler::module_card(path)` to compile a real `ModuleCard` from graph facts.
- Add `EntryPointCard` struct, `EntryPointKind` enum, and `GraphCardCompiler::entry_point_card(scope, budget)`.
- Implement heuristic entry-point detection from graph facts (no new pipeline stage, no LLM).
- Expose `synrepo_entrypoints(scope?, budget?)` as a new MCP tool.
- Keep detection purely graph-derived so it works immediately after `synrepo init` with no additional passes.

**Non-Goals:**
- Full reachability analysis or call-graph traversal for entry-point discovery (future stage-7 work).
- Detection of dynamic dispatch patterns (trait objects, function pointers).
- Entry-point confidence scoring or overlay annotation.
- `CallPathCard` or `synrepo_call_path` (separate change).
- Modifying the structural pipeline to persist entry-point facts.

## Decisions

### 1. Detection at compile time, not pipeline time

Entry-point detection runs inside `GraphCardCompiler::entry_point_card()`, reading graph rows that are already persisted. It does not add a new pipeline stage or persist new rows.

**Why over a pipeline stage:** The signals are trivially derivable from existing data. Adding a stage would introduce new schema rows, migration concerns, and rebuild triggers for a fact that costs microseconds to re-derive. A pipeline stage is appropriate when the derived fact is expensive or used by other stages — neither applies here.

**Why over an async background pass:** We want zero LLM involvement and instant availability after `init`. Background passes imply latency and a freshness state to manage.

### 2. Heuristic taxonomy for EntryPointKind

Four kinds, determined by symbol name and file path patterns applied to `SymbolNode` rows:

| Kind | Detection rule |
|---|---|
| `binary` | `qualified_name == "main"` and file is in `src/bin/` or is `src/main.rs` |
| `cli_command` | symbol in a file whose path contains `cli`, `command`, or `cmd`; name matches `run`, `execute`, `handle`, or is a top-level `pub fn` in such a file |
| `http_handler` | symbol name matches `handle_*`, `serve_*`, `route_*`, or file path contains `handler`, `route`, `router` |
| `lib_root` | `pub fn` at module root (`src/lib.rs` or `mod.rs` at a directory boundary) with no callers inside the same file |

Rules are applied in order; first match wins. Unknown patterns return no entry point for that symbol rather than a spurious result.

**Why heuristics over graph edges:** The graph does not yet have `EntryPoint` edge kind or visibility analysis. Adding them is stage-7 work. Heuristics from existing fields give 90% precision for the common Rust project layout today. False negatives (missed handlers) are safe; false positives are bounded by the pattern strictness.

**Why four kinds:** These cover the distinct agent questions: "where does the binary start", "which CLI verb handles X", "which HTTP route handles X", "what is the library's public API root". More kinds can be added later without breaking the card contract.

### 3. ModuleCard scope = immediate directory

`module_card(path)` scopes to the immediate children of the requested directory: files directly inside, plus their top-level symbols. It does not recurse into subdirectories; subdirectories are listed as `FileRef` entries so agents can request nested module cards explicitly.

**Why not recursive:** Recursive modules produce unbounded card size. The budget tier model requires predictable token counts. Agents can drill down with successive calls.

### 4. Budget tiers for EntryPointCard

| Tier | Content |
|---|---|
| `tiny` | Entry point list: kind, qualified name, file path, line. No symbol body. |
| `normal` | Above plus call-site count (number of unique callers in the graph) and doc comment (truncated to 80 chars). |
| `deep` | Above plus full source signature and, if a `SymbolCard` can be compiled for the entry point, it is inlined. |

ModuleCard budget follows the same shape as FileCard: `tiny` = file list only; `normal` = file list + public symbol names; `deep` = full public symbol details.

### 5. MCP tool: scope parameter as path prefix

`synrepo_entrypoints(scope?, budget?)` accepts an optional `scope` string interpreted as a path prefix (e.g., `src/bin/`, `src/surface/`). When absent, the compiler scans all files for entry points. Results are sorted by kind (binary first) then by file path.

**Why path prefix over node ID:** Agents think in terms of directories and files during orientation, not node IDs. A path prefix is the natural scope for "where does execution start in this subsystem?".

## Risks / Trade-offs

- **Heuristic false positives for http_handler and cli_command:** Pattern matching on names produces noise in non-standard project layouts. Mitigation: keep the pattern list conservative; add a `low_confidence` label on heuristic-detected kinds so agents can filter.
- **Module card staleness after renames:** `module_card(path)` uses the current file list from the graph; if the graph is stale, the card is stale. This is the same staleness model as `FileCard`. No special handling needed.
- **Unbounded result sets for large repos:** `synrepo_entrypoints` with no scope can return many results. Mitigation: default limit of 20 entries per MCP call; agent can narrow with `scope`.
- **Compiler size:** `entry_point_card()` logic may push the compiler module past 400 lines. Plan: detect entry points in a dedicated `entry_point.rs` submodule under `compiler/`.

## Open Questions

- Should `lib_root` detection require `pub` visibility at the Rust level, or rely on the `Export` SymbolKind? `Export` is already emitted by the TypeScript extractor but not consistently for Rust today. Decision deferred to implementation; fall back to name pattern for Rust until `Export` is reliable.
- Should `synrepo_overview` be updated to include a short entry-points summary? Out of scope for this change; revisit when `synrepo_overview` is refactored.
