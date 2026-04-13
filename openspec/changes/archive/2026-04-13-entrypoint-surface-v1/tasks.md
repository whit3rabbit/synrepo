## 1. Types and Traits

- [x] 1.1 Add `EntryPointKind` enum (`Binary`, `CliCommand`, `HttpHandler`, `LibRoot`) to `src/surface/card/types.rs` with `Serialize`/`Deserialize` and `#[serde(rename_all = "snake_case")]`
- [x] 1.2 Add `EntryPoint` struct to `src/surface/card/types.rs` with fields: `symbol`, `qualified_name`, `location`, `kind`, `caller_count` (Option), `doc_comment` (Option), `signature` (Option)
- [x] 1.3 Add `EntryPointCard` struct to `src/surface/card/types.rs` with fields: `scope`, `entry_points`, `approx_tokens`, `source_store`
- [x] 1.4 Re-export `EntryPointCard`, `EntryPoint`, `EntryPointKind` from `src/surface/card/mod.rs`
- [x] 1.5 Add `entry_point_card(scope: Option<&str>, budget: Budget) -> crate::Result<EntryPointCard>` to the `CardCompiler` trait in `src/surface/card/mod.rs`
- [x] 1.6 Add `module_card(path: &str, budget: Budget) -> crate::Result<ModuleCard>` to the `CardCompiler` trait in `src/surface/card/mod.rs`

## 2. EntryPoint Detection

- [x] 2.1 Create `src/surface/card/compiler/entry_point.rs` — define `detect_entry_points(store, scope, budget)` that queries `SymbolNode` rows and applies the four-rule `EntryPointKind` taxonomy from the spec
- [x] 2.2 Implement `binary` detection: `qualified_name == "main"` and file path matches `src/main.rs` or `src/bin/*.rs`
- [x] 2.3 Implement `cli_command` detection: file path segment contains `cli`, `command`, or `cmd`; symbol has `SymbolKind::Function`
- [x] 2.4 Implement `http_handler` detection: symbol name starts with `handle_`, `serve_`, or `route_`; or file path segment contains `handler`, `route`, or `router`
- [x] 2.5 Implement `lib_root` detection: file is `src/lib.rs` or a `mod.rs`; symbol is top-level with no callers from the same file
- [x] 2.6 Apply rule ordering (binary first, then cli_command, http_handler, lib_root); stop at first match per symbol
- [x] 2.7 Sort results: `Binary` < `CliCommand` < `HttpHandler` < `LibRoot`, then by file path; cap at 20 entries

## 3. Budget-Tier Truncation

- [x] 3.1 At `Tiny`: include only `kind`, `qualified_name`, `location`; omit `caller_count`, `doc_comment`, `signature`
- [x] 3.2 At `Normal`: include `caller_count` (query `Calls` edge count from graph) and `doc_comment` truncated to 80 chars
- [x] 3.3 At `Deep`: include full `signature` and inline a compiled `SymbolCard` for each entry point (call `symbol_card()` for each)

## 4. GraphCardCompiler Methods

- [x] 4.1 Implement `GraphCardCompiler::entry_point_card()` in `entry_point.rs` and wire it into the trait `impl` in `src/surface/card/compiler/mod.rs`
- [x] 4.2 Implement `GraphCardCompiler::module_card()` in a new `src/surface/card/compiler/module.rs` — query `FileNode` rows with path prefix, collect immediate children, group by direct vs. nested, collect `public_symbols` from `SymbolNode` rows
- [x] 4.3 Apply ModuleCard budget truncation: `Tiny` = file list + symbol counts only; `Normal` = + symbol names and kinds; `Deep` = + one-line signatures and truncated doc comments
- [x] 4.4 Declare `mod entry_point;` and `mod module;` in `src/surface/card/compiler/mod.rs`

## 5. MCP Tool

- [x] 5.1 Add `EntrypointsParams` struct to `crates/synrepo-mcp/src/main.rs` with fields `scope: Option<String>` and `budget: String` (default `"tiny"`)
- [x] 5.2 Implement `synrepo_entrypoints` tool handler: parse params, call `compiler.entry_point_card(scope.as_deref(), budget)`, serialize to JSON string
- [x] 5.3 Register the tool in the `#[tool_router]` impl block
- [x] 5.4 Update the `get_info` instructions string to mention `synrepo_entrypoints`

## 6. Skill Update

- [x] 6.1 Update `skill/SKILL.md` to add `synrepo_entrypoints` to the current MCP surface listing and remove it from the "not yet shipped" list

## 7. Tests

- [x] 7.1 Add unit tests in `entry_point.rs` for each `EntryPointKind` detection rule (one test per rule, covering match and non-match)
- [x] 7.2 Add a test verifying rule ordering (symbol matching two rules is classified by the first matching rule only)
- [x] 7.3 Add a test for `module_card()` verifying that subdirectory files are excluded from `files` and appear in `nested_modules`
- [x] 7.4 Add a test for `module_card()` with an empty directory (expects empty file list, no error)
- [x] 7.5 Add an integration test in `src/bin/cli_support/tests.rs` or a new test file confirming `entry_point_card` returns an empty list (no panic) when no files are indexed
- [x] 7.6 Run `cargo clippy --workspace --all-targets -- -D warnings` and resolve any new warnings
- [x] 7.7 Run `cargo test` and confirm all tests pass
