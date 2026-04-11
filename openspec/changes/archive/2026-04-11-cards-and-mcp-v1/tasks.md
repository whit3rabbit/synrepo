## 1. Stage 4: cross-file edge resolution

- [x] 1.1 Add tree-sitter queries for call sites in Rust, Python, and TypeScript/TSX; extract `(callee_name, call_site_location)` pairs from `ExtractedSymbol` during parse
- [x] 1.2 Add tree-sitter queries for import/use statements; extract `(imported_path_or_name)` per file during parse
- [x] 1.3 Implement post-parse name-resolution pass in `run_structural_compile`: build a `qualified_name → SymbolNodeId` index after stages 1–3, then emit `Calls` and `Imports` edges for resolved names
- [x] 1.4 Add tests: assert `Calls` and `Imports` edges appear in the graph after compiling a two-file fixture where one calls/imports the other

## 2. CardCompiler implementation

- [x] 2.1 Implement `GraphCardCompiler` struct in `src/surface/card/compiler.rs` (split card.rs into `card/mod.rs` + `card/compiler.rs` if needed to stay under 400 lines); implement `CardCompiler` trait
- [x] 2.2 Implement `symbol_card()`: query symbol node, collect callers/callees via `Calls` edges with budget-aware truncation, include source body for `Deep` budget
- [x] 2.3 Implement `file_card()`: query file node, collect defined symbols via `Defines` edges, collect importers/imports via `Imports` edges
- [x] 2.4 Implement `resolve_target()`: try file path lookup, then qualified name substring match
- [x] 2.5 Define `ModuleCard` struct in `src/surface/card/mod.rs` (struct only, no compiler method yet)
- [x] 2.6 Add insta snapshot tests for `symbol_card` and `file_card` output at all three budget tiers against a fixture graph

## 3. Workspace conversion

- [x] 3.1 Add `[workspace]` table to root `Cargo.toml` with `resolver = "2"` and `members = [".", "crates/synrepo-mcp"]`
- [x] 3.2 Create `crates/synrepo-mcp/` directory with `Cargo.toml` declaring `synrepo` as a path dep and adding `rmcp`, `tokio` (full features) as deps
- [x] 3.3 Confirm `cargo build --workspace` compiles cleanly with no errors

## 4. MCP server

- [x] 4.1 Scaffold `crates/synrepo-mcp/src/main.rs`: open graph store from `.synrepo/graph/`, wrap in `GraphCardCompiler`, start `rmcp` server over stdio
- [x] 4.2 Implement `synrepo_card` tool: resolve target, call `symbol_card` or `file_card`, return JSON
- [x] 4.3 Implement `synrepo_search` tool: delegate to `substrate::search`, return results as JSON
- [x] 4.4 Implement `synrepo_overview` tool: return graph stats + top-level file cards at tiny budget
- [x] 4.5 Implement `synrepo_where_to_edit` tool: search then return file cards at tiny budget
- [x] 4.6 Implement `synrepo_change_impact` tool: collect inbound Imports+Calls edges from target, return cards
- [x] 4.7 Update `skill/SKILL.md` current-phase section and `CLAUDE.md` phase-status table

## 5. Validation

- [x] 5.1 Run `cargo clippy --workspace -- -D warnings` and fix any warnings
- [x] 5.2 Run `cargo test --workspace` and confirm all tests pass
- [x] 5.3 Run `make check` (fmt-check + lint + test) to confirm CI-equivalent passes
