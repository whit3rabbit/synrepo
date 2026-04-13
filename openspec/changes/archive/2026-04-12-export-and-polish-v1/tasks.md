## 1. Go structural language support

- [x] 1.1 Add `tree-sitter-go` grammar crate to `Cargo.toml` and verify it compiles with the existing workspace
- [x] 1.2 Add `SymbolKind::TypeDef` and `SymbolKind::Interface` variants to `src/structure/graph/symbol.rs` (or the file that defines `SymbolKind`); update all match arms and serialization
- [x] 1.3 Create `src/structure/parse/go.rs` implementing the `LanguageAdapter` trait: symbol extraction for functions, methods, types, interfaces, constants, and variables using tree-sitter-go queries
- [x] 1.4 Populate `signature` (declaration up to `{` or end-of-line) and `doc_comment` (preceding `//` comments) in the Go adapter, following the pattern in `src/structure/parse/extract/mod.rs`
- [x] 1.5 Add Go call extraction query to the Go adapter: extract function calls and method calls as `(caller_file, callee_name)` pairs for stage 4 resolution
- [x] 1.6 Add Go import extraction query: extract `import` paths for stage 4 `Imports` edge resolution, mapping Go import paths to discovered file nodes by path suffix
- [x] 1.7 Wire the Go adapter into `src/structure/parse/mod.rs` language dispatch (alongside Rust, Python, TypeScript/TSX)
- [x] 1.8 Add a Go fixture directory at `tests/fixtures/go/` with a small multi-file Go package; add an insta snapshot test for Go symbol extraction covering at least one function, one type, one interface, and one import
- [x] 1.9 Add a grammar validation test that confirms expected symbol kinds and minimum symbol count on the Go fixture; this test must fail if the grammar is upgraded and queries break silently
- [x] 1.10 Run `make check` after Go adapter integration; resolve any lint or snapshot failures

## 2. Export command and manifest

- [x] 2.1 Add `export_dir: String` (default: `"synrepo-context"`) to `Config` in `src/config.rs`; mark it as non-compatibility-sensitive (changing it does not trigger a rebuild)
- [x] 2.2 Create `src/pipeline/export/mod.rs` with `ExportManifest` struct (fields: `graph_schema_version: u32`, `last_reconcile_epoch: u64`, `format: ExportFormat`, `budget: Budget`, `generated_at: DateTime<Utc>`) and `ExportFormat` enum (`Markdown`, `Json`)
- [x] 2.3 Implement `write_exports(compiler: &GraphCardCompiler, config: &Config, format: ExportFormat, budget: Budget) -> Result<ExportManifest>` in `src/pipeline/export/mod.rs`: compile all `FileCard` and `SymbolCard` records at the specified budget, render to the output directory, write manifest JSON
- [x] 2.4 Implement markdown rendering in `src/pipeline/export/render.rs`: one file per card type (`symbols.md`, `files.md`, `decisions.md`) with a generated-file header comment in each
- [x] 2.5 Implement JSON rendering: single `index.json` with card arrays keyed by type
- [x] 2.6 Add gitignore management: if `synrepo-context/` is not in the repo-root `.gitignore`, append it (skip if `--commit` flag is passed)
- [x] 2.7 Add `export` subcommand to `src/bin/cli_support/commands/` with flags: `--format <markdown|json>` (default: markdown), `--deep` (use Deep budget, default: Normal), `--commit` (suppress gitignore insertion), `--out <dir>` (override export_dir from config)
- [x] 2.8 Wire `export` subcommand into `src/bin/cli.rs` dispatch
- [x] 2.9 Tests in `src/pipeline/export/tests.rs`: export produces expected files, manifest records correct epoch, `--commit` suppresses gitignore insertion, `--deep` uses Deep budget

## 3. Export repair surface

- [x] 3.1 Add `RepairSurface::ExportSurface` variant to `src/pipeline/repair/types/stable.rs`; update both `#[serde(rename_all = "snake_case")]` and manual `as_str()` (follow the existing dual-mapping pattern)
- [x] 3.2 Add `RepairAction::RegenerateExports` variant in the same file; update both mappings
- [x] 3.3 Extend `src/pipeline/repair/report.rs` to check the export manifest: if no manifest exists report `absent`; if manifest epoch is behind current reconcile epoch report `stale` with drift class `stale` and action `regenerate_exports`
- [x] 3.4 Extend `src/pipeline/repair/sync.rs` to handle `RegenerateExports`: call `write_exports` using the format and budget recorded in the manifest; update the manifest epoch after success; append resolution log entry
- [x] 3.5 Update stable-identifier tests in `src/pipeline/repair/types/tests.rs` to cover the new `ExportSurface` and `RegenerateExports` variants
- [x] 3.6 Tests: check reports `stale` when manifest epoch is behind current; sync regenerates and updates manifest; check reports `absent` when no manifest; graph store is untouched after export sync

## 4. Upgrade command

- [x] 4.1 Add `upgrade` subcommand to `src/bin/cli_support/commands/` with flag `--apply` (default: dry-run)
- [x] 4.2 Implement dry-run path: load the existing `CompatibilityReport` from each store via the existing evaluator in `src/store/compatibility/`; format and print a plan table (store name, current version, required action, expected outcome)
- [x] 4.3 Implement apply path: for each store execute its compatibility action in dependency order (index before graph before overlay); capture result per store; emit a structured result report
- [x] 4.4 Ensure `block` actions in apply mode produce a clear error message with manual recovery steps and exit non-zero; `continue` actions are no-ops; `rebuild` / `clear-and-recreate` actions delete and reinitialize the store
- [x] 4.5 Wire `upgrade` into `src/bin/cli.rs` dispatch; add `upgrade` to the status and help text
- [x] 4.6 Add version-skew startup warning: in the startup path that opens stores, if any store is outside the supported range emit a warning recommending `synrepo upgrade` before proceeding with the compatibility action
- [x] 4.7 Tests: dry-run prints plan without mutating stores; apply with no skew exits zero; apply with `block`-action store exits non-zero with error message; startup warning is emitted when version is out of range

## 5. agent-setup expansion

- [x] 5.1 Create `src/bootstrap/shim/cursor.rs` with the cursor shim template targeting `.cursor/rules/synrepo.mdc`; content describes available MCP tools at the current shipped surface
- [x] 5.2 Create `src/bootstrap/shim/codex.rs` targeting `.codex/instructions.md`; content describes MCP server configuration and tool list
- [x] 5.3 Create `src/bootstrap/shim/windsurf.rs` targeting `.windsurfrc` or equivalent; follow the windsurf rules file convention
- [x] 5.4 Add `--regen` flag to the `agent-setup` CLI command; implement content comparison (read existing file, compare against rendered template); overwrite and print diff summary if different; exit zero with "already current" if not
- [x] 5.5 Add `cursor`, `codex`, `windsurf` to the accepted target list and dispatch in `src/bin/cli_support/commands/agent_shims.rs` (or wherever agent-setup targets are dispatched)
- [x] 5.6 Update AGENTS.md shipped surface list with new `agent-setup` targets
- [x] 5.7 Tests: cursor shim is written to correct path; `--regen` overwrites a stale shim; `--regen` does not overwrite a current shim; unknown target produces a clear error

## 6. Status output enrichment

- [x] 6.1 Read export manifest freshness in `src/bin/cli_support/commands/` status handler: if manifest exists, include `export: current | stale (since <timestamp>)` in output; if absent, include `export: absent`
- [x] 6.2 Read overlay cost-to-date from the audit tables: count total LLM calls and sum estimated token usage from the cross-link and commentary audit rows; add to status output as `overlay cost: <N> calls, ~<M> tokens (est.)`
- [x] 6.3 Cache the cost-to-date summary in `reconcile-state.json` so `synrepo status` does not perform a full audit table scan on every invocation; invalidate the cache on each reconcile
- [x] 6.4 Update `synrepo status --json` output schema to include `export_freshness` and `overlay_cost_summary` fields
- [x] 6.5 Tests: status output includes export freshness when manifest exists; status output includes cost summary when overlay has entries; status remains fast (no full audit scan) when cache is current

## 7. Config, AGENTS.md, and validation

- [x] 7.1 Update AGENTS.md Gotchas section: add note that `RepairSurface::ExportSurface` and `RepairAction::RegenerateExports` require dual-mapping updates (same pattern as `ProposedLinksOverlay` / `RevalidateLinks`)
- [x] 7.2 Update AGENTS.md Phase status section: add Go to supported structural languages; update shipped surface to include `export`, `upgrade`, new `agent-setup` targets, and enriched `status`
- [x] 7.3 Update AGENTS.md Active changes section to reference `export-and-polish-v1` while work is in progress (set to "none" after archive)
- [x] 7.4 Run `make check` (fmt-check + clippy + test) across all changes; resolve any lint or test failures before marking tasks complete
- [ ] 7.5 After implementation is complete and tests pass, run `/opsx:archive` to archive the change and sync delta specs into the main specs tree
