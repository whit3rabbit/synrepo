## Context

Stage 4 of the structural compile resolves cross-file edges (`Calls`, `Imports`) from parser-produced `ExtractedCallRef` and `ExtractedImportRef` records. The current `resolve_import_ref` dispatch at `src/pipeline/structural/stage4.rs:148` handles TypeScript/TSX (relative paths with candidate extensions) and Python (dotted-name to slash-path). Rust and Go `import_refs` flow through the pipeline unchanged and are silently dropped at lookup time because no branch matches their shape.

For Rust, the silent-drop is not just a missing resolver branch: the current `RUST_IMPORT_QUERY` at `src/structure/parse/language.rs:219-222` captures only the terminal `identifier` child of a `use_declaration`'s `argument`. A query pattern like:

    (use_declaration argument: (scoped_identifier name: (identifier) @import_ref))

captures `HashMap` from `use std::collections::HashMap`, not `std::collections::HashMap`. Last names are not file paths. Resolving a bare identifier to a `FileNodeId` would require a separate symbol-to-file lookup — which is the `Calls` edge's job, not the `Imports` edge's. To emit file-to-file `Imports` edges, the parser must first hand stage 4 a full path.

For Go, the existing `GO_IMPORT_QUERY` already captures the interpreted_string_literal of each `import_spec`, yielding module-qualified strings like `"github.com/user/proj/pkg/foo"`. Resolution requires translating that string into repo-relative paths: read `go.mod`, extract `module <prefix>`, strip the prefix from matching imports, then enumerate `.go` files in the remaining directory. Packages span multiple files, so one import fans out to N edges.

Both resolutions run inside the caller's open transaction (stages 1–4 are atomic), and the existing `file_index: HashMap<String, FileNodeId>` at `stage4.rs:73` is the only lookup primitive needed. Structural shape and hot-path cost are both minimal.

## Goals / Non-Goals

**Goals:**

- Emit `Imports` file→file edges for Rust `use` declarations that reference in-crate paths (`crate::`, `self::`, `super::`, bare module paths that resolve under the crate root).
- Emit `Imports` file→file edges for Go import statements whose module prefix matches the local `go.mod` module declaration, fanning out across all `.go` files in the target package directory.
- Preserve the existing "unresolved = skip silently" contract for external crates and external Go modules.
- Keep stage 4's per-compile cost small — one `go.mod` read per compile, not per import_ref.
- Update `structural-parse` spec scenarios to specify the new contracts precisely, retiring the Rust last-name-skipping scenario.

**Non-Goals:**

- Cross-module Rust symbol references (e.g., file→symbol edges from `use crate::foo::bar` to the `bar` symbol specifically). Scope is file→file only.
- Other edge kinds (`Inherits`, `References`, `Mentions`). Still phase-1 boundary for every language.
- New language grammars (JS, Java, C#, C/C++). Each is a separate change.
- Deep Rust module-tree logic (attributes like `#[path = "..."]`, out-of-tree `mod` declarations, build.rs-generated files). Skip silently when heuristic candidates miss.
- Go `replace` / `vendor` directives. Rely solely on `module <prefix>` from `go.mod`.
- Config-driven override for alternate Rust crate roots or Go module roots. The repo root from `CompileContext` is authoritative.

## Decisions

### D1: Reshape Rust import query to capture full scoped_identifier text

Replace:

    (use_declaration argument: (identifier) @import_ref)
    (use_declaration argument: (scoped_identifier name: (identifier) @import_ref))

with:

    (use_declaration argument: (identifier) @import_ref)
    (use_declaration argument: (scoped_identifier) @import_ref)

The `@import_ref` capture on a `scoped_identifier` node yields its full span text, so `node_text` in `extract_import_refs` at `src/structure/parse/extract/mod.rs:305` returns `std::collections::HashMap` rather than `HashMap`. The bare-identifier arm is kept to handle `use foo;` (single-segment imports).

**Rationale.** Simplest possible reshape. No changes to extractor or extractor tests beyond updated expected values. Tree-sitter query engine handles the node-text capture uniformly.

**Alternatives considered:**
- *Add a separate `@use_path` capture alongside `@import_ref`*: rejected — doubles capture plumbing in the extractor to no benefit; `module_ref` already expects one field.
- *Post-process last-name captures by walking back through parse-tree ancestors*: rejected — re-introduces parse-tree dependency in stage 4, breaking the parser/resolver boundary.

### D2: Rust resolution via candidate-path enumeration, no filesystem introspection

Given a use-path and the importing file's repo-relative path, produce candidate target paths:

1. Strip one leading prefix (`crate::` | `self::` | `super::`). Reject paths beginning with any other identifier that doesn't match a first-party crate name (for v1, reject all of them — they're third-party).
2. For `crate::`: target is rooted at the crate's `src/` directory. Find the crate root by walking up from the importing file until a `Cargo.toml` sibling is found, then append `src/` (or use the `[lib] path` / `[[bin]] path` entries if present — defer to "skip silently" if parsing Cargo.toml is non-trivial).
3. For `self::`: target is rooted at the importing file's parent directory.
4. For `super::`: walk one directory up per `super::` prefix, then apply remainder rules.
5. For bare paths (e.g., `foo::bar::Thing`): treat as crate-relative only if the first segment matches a module in the current crate (heuristic: does `src/foo/` or `src/foo.rs` exist in `file_index`?). Otherwise skip (third-party).
6. From the resolved base path, map remaining `::` to `/`, then enumerate candidates:
   - `<base>.rs`
   - `<base>/mod.rs`
   Drop the last segment and retry:
   - `<base_without_last>.rs`
   - `<base_without_last>/mod.rs`
   (This handles `use crate::module::Thing` where `Thing` is a symbol inside `module.rs`, not a file.)

`resolve_import_ref` returns every candidate that exists in `file_index`. The existing caller already picks the first match and skips on miss (`stage4.rs:111-113`). If the use imports a sub-item (`use crate::foo::bar::Baz`), both `crate::foo::bar` and `crate::foo` may exist as files; pick the *longest* path that exists to favour the most specific module.

**Rationale.** Pure repo-path arithmetic against the in-memory `file_index`. No fs reads, no Cargo.toml parsing in v1. The "skip silently" contract covers the inevitable miss cases (out-of-tree `mod`, `#[path]`, workspace crates) without failing compile.

**Alternatives considered:**
- *Parse `Cargo.toml` to locate crate roots and workspace members*: rejected for v1 — adds `toml` dep (already transitively present, but not in our direct tree for this module), enlarges the change surface, and the heuristic "look for a sibling Cargo.toml" handles the common case.
- *Use the graph's existing module structure (resolved `mod` declarations)*: rejected — the graph stores `Defines` edges for symbols but not a module tree; we'd have to build the tree fresh each compile.

### D3: Go resolution via one-shot `go.mod` read plus directory fan-out

On `run_cross_file_resolution` entry, compute a `ResolverContext`:

    struct ResolverContext {
        repo_root: PathBuf,
        go_module_prefix: Option<String>,
    }

Populate `go_module_prefix` by reading `<repo_root>/go.mod`, scanning for a line starting with `module ` (whitespace-trimmed), and capturing the remainder. On read failure or missing `go.mod`, leave `None`.

Thread `&ResolverContext` into `resolve_import_ref`. The Go branch:

1. Return empty if `go_module_prefix` is `None`.
2. Check that the import string starts with `<prefix>/` or equals `<prefix>`. If not, return empty (external module).
3. Strip the prefix. The remainder is a repo-relative directory.
4. Enumerate every `.go` file in `file_index` whose key starts with `<remainder>/` and has exactly one `/` between `<remainder>` and the filename (excludes deeper subdirectories, which are distinct packages).
5. Return all matches as candidates. The caller's existing loop at `stage4.rs:111-113` picks the first, but for Go we need fan-out — see D4.

**Rationale.** `go.mod` is tiny (typically < 50 lines); a single read per compile is negligible. Directory fan-out mirrors how Go packages work: `import "prefix/pkg/foo"` refers to every `.go` file declaring `package foo` in `pkg/foo/`.

**Alternatives considered:**
- *Read every file's package declaration and build a package→files index*: rejected — requires re-parsing Go files specifically for package names, duplicating parser work.
- *Defer directory enumeration to a secondary pass*: rejected — stage 4 already has `file_index` and the hot path is an in-memory string scan.

### D4: Fan-out for Go requires caller loop change, not resolver change

`resolve_import_ref` returns `Vec<String>` and the caller picks the first existing match. For Go, every candidate is a real file, and all should emit edges. Two options:

- **Option A**: Caller changes behaviour per language — for Go, emit an edge for every candidate; for others, pick first. This requires stage 4 to know the importing file's language.
- **Option B**: Caller emits edges for *all* candidates that exist in `file_index`, always. For TS/Python, the current `find_map(|p| file_index.get(&p).copied())` picks the first match because candidates are ordered and only one is expected to exist; switching to `filter_map` is harmless because duplicates would be deduped by the `derive_edge_id` hash at insert time anyway.

Pick **Option B**. Simpler caller, language-agnostic. The edge table's unique key on `(from, to, kind)` (via `derive_edge_id`) means a duplicate emission from overlapping TS candidates is an idempotent re-insert, not a dupe row. Verify this property holds against the existing TS edge-emission test; if it does, ship it. If it doesn't, fall back to Option A.

### D5: ResolverContext plumbing

`run_cross_file_resolution` signature becomes:

    pub fn run_cross_file_resolution(
        graph: &mut dyn GraphStore,
        pending: &[CrossFilePending],
        revision: &str,
        repo_root: &Path,
    ) -> crate::Result<usize>

The function builds `ResolverContext` internally. Callers in `src/pipeline/structural/` need updating to pass the repo root, which is already available from `CompileContext` / the existing orchestrator at `src/pipeline/structural/mod.rs`. No public API change outside `pipeline::structural`.

`resolve_import_ref` becomes:

    fn resolve_import_ref(
        module_ref: &str,
        importing_file: &str,
        ctx: &ResolverContext,
    ) -> Vec<String>

The TS/Python branches ignore `ctx`; the Rust and Go branches use it.

## Risks / Trade-offs

- **Rust query reshape breaks latent parse-layer assertions** → Mitigation: run `cargo test --lib structure::parse::` after the query change but before resolver changes. If `qualname_tests` or `malformed_tests` break in unexpected ways, isolate the regression before touching stage 4. The `validation_tests::import_queries_compile_and_expose_import_ref` test should catch any lost `@import_ref` capture.

- **Rust candidate enumeration false-positives emit wrong edges** → Mitigation: the "candidate must exist in `file_index`" gate means bad candidates silently drop. The worst case is a missing edge, not a wrong edge. Integration test in `edges.rs` pins the happy path; if false-positives appear in smoke tests, tighten the bare-path branch (D2 step 5) with a stricter first-segment check.

- **Go directory fan-out inflates edge count on large monorepos** → Mitigation: fan-out is bounded by files-per-package. Typical Go packages hold 1–10 files. Emit count is observable via `synrepo graph stats`; if it becomes a problem, reduce fan-out to one edge per (importing file, target package directory) by picking a canonical representative file.

- **`ResolverContext` plumbing leaks through the stage-4 public signature** → Mitigation: the signature is already implementation-layer (`pub(crate)` in effect — called only from `pipeline::structural::mod.rs`). Library consumers of `synrepo` cannot call stage 4 directly. No external API breakage.

- **Option B (fan-out for all candidates) changes TS edge emission semantics** → Mitigation: the derive_edge_id idempotence property must hold. If the existing TS edges test regresses with Option B, fall back to Option A and add per-language dispatch in the caller. Treated as an open question below.

- **Rust crate-root detection via "walk up to Cargo.toml" misses workspace crates** → Mitigation: for v1, the heuristic "walk up until a directory contains Cargo.toml" finds the nearest crate manifest and treats its `src/` as the root. Workspace roots (which also have Cargo.toml but no src/) would resolve incorrectly; check for `src/` existence before committing to a crate root, and skip silently if none. Improvement deferred.

## Migration Plan

1. Land the Rust `RUST_IMPORT_QUERY` reshape first. Run parse-layer tests to confirm no regressions beyond the `import_refs` expected-value updates in `refs_tests.rs`.
2. Land the `ResolverContext` signature change and TS/Python passthrough (no behaviour change for existing languages). Confirm `edges.rs` existing tests stay green.
3. Land the Go branch. Add integration test. Smoke-test against a real Go repo with `go.mod`.
4. Land the Rust branch. Add integration test. Smoke-test against `synrepo` itself.
5. Update `structural-parse` spec as the final step so the delta specs match shipped behaviour exactly.

No runtime migration. Stage 4 is idempotent per compile; existing graphs pick up new `Imports` edges on the next `synrepo init` or `synrepo reconcile`. No storage format change, no compat advisory.

**Rollback.** Revert the commit. Graphs compiled under the new behaviour will have extra `Imports` edges that the old binary does not produce; the old binary's compile pass will not re-emit them, but it also will not delete them. Resident edges become inert without active re-observation. A user who needs a clean rollback can `synrepo init` under the old binary to fully re-parse. Not a hard requirement.

## Open Questions

- **O1**: Is `derive_edge_id` idempotent under duplicate `(from, to, kind)` insertion? Needs verification before committing to Option B in D4. If not idempotent, switch to Option A (per-language caller dispatch).
- **O2**: Should bare Rust use-paths (`use foo::bar::Thing`, no `crate::` prefix) be treated as third-party and skipped, or as crate-relative when `foo` matches a top-level module? D2 step 5 picks the latter as heuristic; confirm with a smoke-test that false-positive rate is acceptable. If not, tighten to "only resolve paths with explicit `crate::` / `self::` / `super::` prefixes" for v1.
- **O3**: Should Go `internal/` visibility rules be respected (i.e., an import of `a/internal/b` from outside `a/` is invalid Go)? v1 ignores this — resolves by path alone. If it surfaces as a real problem, add a visibility filter.
