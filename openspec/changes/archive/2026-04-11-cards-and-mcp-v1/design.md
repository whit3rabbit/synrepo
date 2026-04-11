## Context

Milestones 0–2 are complete: the graph is bootstrapped automatically, kept fresh via reconcile, and inspectable via raw node/edge CLI commands. Milestone 3 is the card and MCP surface. The design specs define the contracts (`openspec/specs/cards/spec.md`, `openspec/specs/mcp-surface/spec.md`); this document records the implementation decisions for `cards-and-mcp-v1`.

## Goals / Non-Goals

**Goals:**
- Emit `Calls` and `Imports` edges during the structural pipeline using name-based resolution.
- Implement `CardCompiler` for `SymbolCard`, `FileCard`, and `ModuleCard` backed by the existing `SqliteGraphStore`.
- Convert to a Cargo workspace without moving any library code.
- Expose the five core MCP tools via a new `synrepo-mcp` binary crate using `rmcp`.

**Non-Goals:**
- Exact (type-aware) cross-file resolution. Name-based approximation is the phase-1 contract.
- Git-intelligence enrichment of cards (`last_change`, `co_changes`). Stubs return `None` until `git-intelligence-v1`.
- Overlay commentary in cards. `overlay_commentary` stays `None` until phase 4.
- Card persistence or caching across sessions.
- Watch-mode MCP: the MCP server opens a fresh graph connection on each request.

## Decisions

### Stage 4: cross-file edge resolution

Name-based approximate resolution: after stages 1–3 populate all file and symbol nodes, a post-parse pass builds an in-memory name index (`qualified_name → SymbolNodeId`). For each symbol's call sites (extracted via tree-sitter query), the pass looks up the callee name in the index. Matched pairs emit `Calls` edges with `epistemic: ParserObserved`. For import statements, matched file paths emit `Imports` edges. Unresolved names are silently skipped (no error, no placeholder edge).

Alternative considered: full import graph + type inference. Rejected as massively out of scope for phase 1; approximate matching gives 80% of the value for 5% of the effort.

Alternative considered: store unresolved call sites as `References`. Rejected for now; adds noise to the graph without adding resolution.

### CardCompiler implementation

`GraphCardCompiler` is a new struct in `src/surface/card.rs` (or a new `src/surface/card/compiler.rs` sub-module if card.rs grows past 400 lines) that takes a reference to `dyn GraphStore`. It implements the `CardCompiler` trait:

- `symbol_card`: query node by ID, collect `Calls`/inbound `Calls` edges for callees/callers (up to 5 each for tiny, all for normal/deep), include source body only for deep.
- `file_card`: query file node, collect `Defines` edges for symbols, `Imports`/inbound `Imports` edges.
- `resolve_target`: try path lookup first, then qualified name substring match.
- Token estimation: 1 token ≈ 4 bytes of serialized JSON. Use `approx_tokens = serde_json::to_string(&card)?.len() / 4`.
- Budget truncation order: for `tiny`, truncate callees/callers to 3, omit doc_comment, omit source_body. For `normal`, include full callers/callees, include doc_comment. For `deep`, include source_body by reading the file from disk.

### ModuleCard

`ModuleCard` is a new type in `src/surface/card.rs`: describes a directory/module by listing its top-level files (via `FileRef` slice) and any symbols with module-level visibility. Not included in `CardCompiler` trait for this change; defined as a struct only.

Alternative considered: skip ModuleCard. Rejected because it's referenced in spec and CLAUDE.md; defining the struct establishes the pattern even if the compiler method comes later.

### Workspace conversion

Add a `[workspace]` table to the root `Cargo.toml` with `members = [".", "crates/synrepo-mcp"]`. Create `crates/synrepo-mcp/Cargo.toml` with only the MCP-specific deps (`rmcp`, `tokio`, `synrepo`). No files move; the library crate stays at the repo root.

Workspace resolver: `resolver = "2"`.

Per CLAUDE.md: `rmcp` is `modelcontextprotocol/rust-sdk` on crates.io. Verify the published crate name before adding.

### MCP server

Five tools implemented as async functions in `crates/synrepo-mcp/src/`:

1. `synrepo_card(target: str, budget: "tiny"|"normal"|"deep")` → resolves target via `CardCompiler::resolve_target`, calls `symbol_card` or `file_card`.
2. `synrepo_search(query: str, limit: u32)` → delegates to `substrate::search`.
3. `synrepo_overview()` → returns graph stats + top-level files via `file_card` at tiny budget.
4. `synrepo_where_to_edit(task: str, limit: u32)` → runs `synrepo_search` then wraps results in FileCards at tiny budget.
5. `synrepo_change_impact(target: str)` → collects inbound `Imports`+`Calls` edges from `target`, returns callers/importers as tiny FileCards and SymbolCards.

Server startup: open `SqliteGraphStore` from `.synrepo/graph/`, wrap in `GraphCardCompiler`, serve over stdio (MCP default transport).

## Risks / Trade-offs

- Name-based call resolution will produce false positives for overloaded names. Documented in gotchas; acceptable for phase 1.
- Workspace conversion requires verifying `rmcp` is available on crates.io before adding. If not, gate behind a git dep.
- MCP server opens graph read-only; if a reconcile is in progress, it reads a partially-written state. Acceptable because readers don't acquire the write lock; the worst case is a slightly stale card.

## Migration Plan

1. Wire stage 4 into structural compile; run `cargo test` to confirm no regressions.
2. Implement `GraphCardCompiler` with unit tests for budget truncation.
3. Convert to workspace; confirm `cargo build --workspace` passes.
4. Implement MCP server; test tools manually against a bootstrapped repo.
5. Update `CLAUDE.md` phase-status table and `skill/SKILL.md` current-phase section.
