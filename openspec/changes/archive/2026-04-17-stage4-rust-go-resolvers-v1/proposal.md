## Why

Stage-4 cross-file `Imports` resolution is wired for TypeScript/TSX and Python only. Rust `use` declarations and Go `import` statements are captured by the parsers but silently dropped at resolution, so cards for Rust and Go repos have systematically thinner cross-file wiring than TS/Python repos — a coverage bug, not a correctness bug. This gap must close before adding more languages, because new grammars added on top of a leaky resolver would reproduce the same silent-drop pattern and dilute the trust signal cards rely on.

## What Changes

- Reshape `RUST_IMPORT_QUERY` in `src/structure/parse/language.rs` to capture the full `use`-path text (e.g., `crate::module::Thing`, `std::collections::HashMap`) via the `scoped_identifier` node, replacing the current last-name capture. **BREAKING** for the parser-layer `ExtractedImportRef.module_ref` contract on Rust: downstream consumers now see full paths, not bare identifiers. Update parse-layer tests accordingly.
- Extend `resolve_import_ref` in `src/pipeline/structural/stage4.rs` with a Rust branch that maps `crate::`, `self::`, and `super::` prefixes plus `::`-separated paths to candidate `.rs` and `mod.rs` files. External crate paths (`std::`, third-party crates) remain unresolved, skipped silently per the existing contract.
- Extend `resolve_import_ref` with a Go branch that reads `go.mod` from the repo root once per compile cycle, extracts the `module <prefix>` line, strips that prefix from matching import strings, and resolves the remainder to every `.go` file in the target directory (Go packages span multiple files). Imports whose module prefix does not match the local `go.mod` are skipped silently (external modules).
- Thread a per-compile `ResolverContext` (repo root path plus cached Go module prefix) through `run_cross_file_resolution` to avoid re-reading `go.mod` on every import_ref.
- Update `src/structure/parse/refs_tests.rs` Rust import_refs assertions to expect full paths, add a `super::` / `crate::` edge case, and extend Go coverage if needed.
- Add stage-4 integration tests in `src/pipeline/structural/tests/edges.rs` asserting Rust `Imports` edges between `crate::`-wired modules and Go `Imports` edges fanning out across a package's `.go` files.
- Update the `structural-parse` spec: replace the scenario that locks Rust last-name skipping as intentional phase-1 behavior with scenarios that specify the new Rust and Go resolution contracts.

## Capabilities

### New Capabilities

None. This change refines an existing spec's requirements rather than introducing a new contract surface.

### Modified Capabilities

- `structural-parse`: Rust `use` last-name skipping is replaced with full-path resolution to `.rs` / `mod.rs` candidates; Go import resolution is introduced using `go.mod` module-prefix stripping with per-directory `.go` fan-out. The parser-layer Rust `import_ref` contract tightens from "last name of use path" to "full use path".

## Impact

- **Code**:
  - `src/structure/parse/language.rs` — Rust import query reshape.
  - `src/pipeline/structural/stage4.rs` — per-compile `ResolverContext`, Rust and Go branches in `resolve_import_ref`, `go.mod` read and cache.
  - `src/structure/parse/refs_tests.rs` — Rust import_refs expectations updated; Go coverage extended.
  - `src/pipeline/structural/tests/edges.rs` — new Rust and Go stage-4 integration tests.
  - Possibly `src/structure/parse/qualname_tests.rs` if the Rust query reshape affects any symbol extraction (verify, do not pre-edit).
- **APIs**: `ExtractedImportRef.module_ref` for Rust now carries full use paths. No type changes. No external (CLI / MCP) surface changes.
- **Dependencies**: None added. `go.mod` parsing is a single-line regex or substring scan; no new crate required.
- **Systems**: No migration. Stage 4 runs inside the same transaction as stages 1–3 (per existing contract); cross-file edges from re-parsed Rust and Go files are emitted on the next `synrepo init` or `synrepo reconcile`. Existing graphs silently gain edges on the next compile cycle without a rebuild.
- **Docs**: `CLAUDE.md` gotchas section needs the Rust-import-query comment updated; the "Stage 4 cross-file edges are now emitted" line needs Rust and Go added alongside TS and Python (deferred to the implementation PR).
