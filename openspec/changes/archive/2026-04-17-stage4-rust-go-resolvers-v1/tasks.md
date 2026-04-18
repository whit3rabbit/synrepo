## 1. Rust import query reshape

- [x] 1.1 Update `RUST_IMPORT_QUERY` in `src/structure/parse/language.rs` so `@import_ref` captures full `scoped_identifier` text, keeping the bare-identifier arm for single-segment `use foo;`.
- [x] 1.2 Update the documentation comment above `RUST_IMPORT_QUERY` to describe the new full-path capture contract.
- [x] 1.3 Run `cargo test --lib structure::parse::validation_tests::` and confirm `import_queries_compile_and_expose_import_ref` still passes.
- [x] 1.4 Update Rust expectations in `src/structure/parse/refs_tests.rs::rust_import_refs_capture_last_name_of_use_paths` to expect full paths; rename the test accordingly (e.g., `rust_import_refs_capture_full_use_paths`).
- [x] 1.5 Add `super::` / `crate::` edge-case coverage to `src/structure/parse/refs_tests.rs`.
- [x] 1.6 Run `cargo test --lib structure::parse::` and fix any unrelated regression before continuing.

## 2. ResolverContext plumbing

- [x] 2.1 Add `struct ResolverContext { repo_root: PathBuf, go_module_prefix: Option<String> }` in `src/pipeline/structural/stage4.rs`.
- [x] 2.2 Add `fn load_go_module_prefix(repo_root: &Path) -> Option<String>` that reads `<repo_root>/go.mod`, scans for a whitespace-trimmed line starting with `module `, and returns the remainder. Return `None` on any read failure or missing file.
- [x] 2.3 Extend `run_cross_file_resolution` signature with a `repo_root: &Path` parameter; build `ResolverContext` from it once before the pending loop.
- [x] 2.4 Update the caller(s) of `run_cross_file_resolution` in `src/pipeline/structural/mod.rs` (and elsewhere under `src/pipeline/structural/`) to pass the repo root already available from `CompileContext`.
- [x] 2.5 Change `resolve_import_ref` signature to accept `&ResolverContext`; TS and Python branches ignore the new parameter.
- [x] 2.6 Run `cargo test --lib pipeline::structural::tests::edges::` and confirm existing TS/Python edge tests stay green with only the signature change applied.

## 3. Verify derive_edge_id idempotence for fan-out

- [x] 3.1 Write a throwaway unit test inserting the same `(from, to, kind)` twice through `graph.insert_edge` and read back; confirm edge count is 1, not 2.
- [x] 3.2 If idempotent, proceed with Option B (caller emits for every existing candidate). If not idempotent, switch the caller loop to per-language dispatch (Option A) and document the decision in a code comment. _Decision: idempotence confirmed, but picked per-language dispatch (Option A) because Rust's spec scenarios require "longest-matching single candidate", not fan-out. Go fans out; Rust / TS / Python pick the first existing candidate. Documented in `stage4.rs` caller loop._
- [x] 3.3 Change the import-resolution caller loop at `stage4.rs:109-138` to emit edges for every candidate that exists in `file_index`, not only the first. _Applied only to Go (fan-out) per 3.2 decision; others keep "first existing"._
- [x] 3.4 Delete the throwaway unit test once the decision is recorded.

## 4. Go resolver

- [x] 4.1 In `resolve_import_ref`, add a Go branch: return empty if `ctx.go_module_prefix` is `None`; otherwise strip the prefix from matching imports and return the remaining repo-relative package directory.
- [x] 4.2 Enumerate `file_index` keys whose path starts with `<remainder>/` and contains exactly one `/` separator between remainder and filename (excludes sub-package directories); collect every `.go` match.
- [x] 4.3 Add an integration test in `src/pipeline/structural/tests/edges.rs`: fixture of two Go packages under a minimal `go.mod`, with `a/a.go` and `a/a_util.go` both `package a`, plus `b/b.go` importing `<module>/a`; assert `Imports` edges emitted from `b/b.go` to both files in `a/`.
- [x] 4.4 Add a negative test: external import `import "fmt"` produces no edges.
- [x] 4.5 Run `cargo test --lib pipeline::structural::tests::edges::`.

## 5. Rust resolver

- [x] 5.1 In `resolve_import_ref`, add a Rust branch that accepts full paths, handles `crate::`, `self::`, and `super::` prefixes per design D2, and produces candidate paths.
- [x] 5.2 Implement crate-root detection: walk up from `importing_file` until a directory contains `Cargo.toml`; treat `<crate_dir>/src/` as the crate root. Require `src/` to exist; otherwise return empty (likely a workspace root).
- [x] 5.3 Implement the candidate enumeration per D2 step 6: `<base>.rs`, `<base>/mod.rs`, then `<base_without_last>.rs`, `<base_without_last>/mod.rs`.
- [x] 5.4 Treat bare paths (no `crate::` / `self::` / `super::` prefix) as crate-relative only if the first segment matches a top-level directory or file under the crate root; skip silently otherwise.
- [x] 5.5 When multiple candidates exist in `file_index`, prefer the longest-matching path to bias towards the most specific module.
- [x] 5.6 Add integration tests in `src/pipeline/structural/tests/edges.rs`:
  - Two-file Rust fixture with `src/a.rs` declaring `pub struct A` and `src/b.rs` containing `use crate::a::A;` — assert an `Imports` edge from `b.rs` to `a.rs`.
  - Nested module fixture with `src/foo/mod.rs` and `src/foo/bar.rs`, plus `src/main.rs` doing `use crate::foo::bar::Thing` — assert edge to `bar.rs`.
  - `super::` fixture: `src/foo/a.rs` with `use super::b::X;` where `src/foo/b.rs` exists — assert edge.
- [x] 5.7 Add a negative test: `use std::collections::HashMap;` in isolation produces no edge.
- [x] 5.8 Run `cargo test --lib pipeline::structural::tests::edges::`.

## 6. Spec delta alignment

- [x] 6.1 Run `openspec validate stage4-rust-go-resolvers-v1 --strict` and fix any spec-format issues.
- [x] 6.2 Verify the modified requirement in `specs/structural-parse/spec.md` matches implemented behaviour exactly: all scenarios pass as-written against the shipped resolver.

## 7. End-to-end verification

- [x] 7.1 Run `make check` and ensure fmt, clippy, and the full test suite pass. _One known-flaky test (`cli_support::tests::compact::compact_apply_library`) failed under parallel writer-lock contention but passed in isolation; documented in AGENTS.md gotcha._
- [x] 7.2 `cargo run -- init` against the synrepo repo itself; confirm `cargo run -- graph stats` reports a non-zero count of `Imports` edges attributable to Rust (baseline before this change was zero from Rust's contribution). _273 imports edges across 283 Rust files on a clone of synrepo itself._
- [x] 7.3 Smoke-test against an external Go repo with a `go.mod` file (any small open-source Go project); confirm non-zero Go `Imports` edges. _cc-supervisor (770 Go files): 8629 imports edges._
- [x] 7.4 Inspect a sample of emitted edges with `cargo run -- node <file_id>` or `cargo run -- graph query "outbound <file_id> imports"` and spot-check that at least five edges point to genuinely imported files. _Go: `cmd/supervibzer/main.go` fans out to every `.go` file under `cmd/supervisor/cmd/` as expected from its `github.com/whit3rabbit/supervibzer/cmd/supervisor/cmd` import. Rust (synrepo self-smoke): 8/8 inspected edges resolve to legitimate sibling/neighbor modules (`shims.rs`→`doctrine.rs`, `cli_args.rs`→`agent_shims/mod.rs`, etc.)._

## 8. Documentation

- [x] 8.1 Update the "Stage 4 cross-file edges are now emitted" gotcha in `AGENTS.md` (synced to `CLAUDE.md` via symlink) to add Rust and Go alongside TS and Python.
- [x] 8.2 Update the `RUST_IMPORT_QUERY` comment block in `src/structure/parse/language.rs` (covered in 1.2) if anything is still stale after implementation. _Comment already accurate after task 1.2 — no stale text remains._
- [x] 8.3 Add a short entry under the phase-status section of `AGENTS.md` noting stage-4 Rust and Go resolution shipped in `stage4-rust-go-resolvers-v1`.
