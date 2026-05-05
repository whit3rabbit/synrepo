# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code) and other coding agents when working with code in this repository.

> `CLAUDE.md` is a symlink to `AGENTS.md`. Edit `AGENTS.md`; both update.

## When working on...

| Topic | Read |
|-------|------|
| Layer architecture, pipeline stages, storage layout, snapshot rules | `docs/ARCHITECTURE.md` |
| On-disk data model: tables, columns, format versions, storage invariants | `docs/SCHEMA.md` |
| Config fields and defaults | `docs/CONFIG.md` |
| MCP workflow, tool surface, resources, agent integration behavior | `docs/MCP.md` |
| Explain providers, API keys, telemetry | `docs/EXPLAIN.md` |
| Adding a new tree-sitter language | `docs/ADDING-LANGUAGE.md` |
| Full foundational design (data model, trust model, evaluation) | `docs/FOUNDATION.md` |

Gotchas and hard invariants stay in this file — read them first.

## Codebase context

When `.synrepo/` exists and synrepo MCP tools are available, use them before reading source files cold:
- Start with repository orientation, then search or find candidate files and symbols.
- Use bounded cards or minimum context to choose files before opening full source.
- Check impact or risks before non-trivial edits, and changed/test guidance before claiming done.
- If MCP tools are unavailable, use the `synrepo` CLI fallback (`synrepo status`, `synrepo search`, `synrepo node`, `synrepo graph query`) instead of blocking.

## CI / Release

Workflows live in `.github/workflows/`: `ci.yml` (push/PR) and `release.yml` (tag trigger).

Secrets required in **this repo only** (Settings > Secrets and variables > Actions):
- `CARGO_REGISTRY_TOKEN` — crates.io token (scopes: publish-new, publish-update)
- `HOMEBREW_TAP_TOKEN` — GitHub PAT with repo scope on `whit3rabbit/homebrew-tap`

Homebrew tap is a sibling repo at `../homebrew-tap/`; cask template is at `packaging/homebrew/Casks/synrepo.rb`.

**Gotcha:** macOS Intel runner is `macos-15-intel` (not `macos-13` — deprecated Dec 2025).

## Commands

```bash
cargo build                        # build
cargo test                         # run all tests
cargo test <test_name>             # run a single test (substring match)
cargo test -p synrepo <test_name>  # run a single test by exact path
cargo test --bin synrepo <test_name>  # run binary-crate tests (cli_support::tests::*)
cargo test --test mutation_soak -- --ignored --test-threads=1  # release-gate crash/contention soak suite (Unix only, serial)
cargo clippy --workspace --bins --lib -- -D warnings  # CI lint gate (product targets)
cargo fmt                          # format
make check                         # local fmt-check + lint + parallel test
make ci-lint                       # CI lint gate, excludes pre-existing test-only clippy debt
make ci-test                       # CI workspace tests, forced serial to avoid parallel writer-lock flakes
make ci-check                      # CI-equivalent fmt-check + lint + serial workspace test
make soak-test                     # run the ignored mutation-surface soak suite serially
cargo run --                       # bare entrypoint: probe + route to dashboard/setup/repair wizard (TTY) or plain-text summary (pipe)
cargo run -- dashboard             # explicit poll-mode dashboard; non-zero exit on uninitialized/partial repos
cargo run -- dashboard --no-color  # dashboard without ANSI styling (still TTY; honored by every TUI entrypoint)
cargo run -- init                  # initialize .synrepo/ in cwd
cargo run -- [--repo <path>] init  # override repo root
cargo run -- reconcile             # refresh graph store without full re-bootstrap
cargo run -- check                 # read-only drift report: surfaces, severities, recommended actions
cargo run -- check --json          # machine-readable JSON drift report
cargo run -- sync                  # repair auto-fixable drift surfaces; appends to .synrepo/state/repair-log.jsonl
cargo run -- sync --json           # JSON sync summary
cargo run -- status [--json]        # operational health: mode, counts, last reconcile, lock, export freshness
cargo run -- task-route <task> [--path <path>] [--json]  # read-only fast-path route classifier
cargo run -- export [--format markdown|json] [--deep] [--commit] [--out <dir>]  # generate synrepo-context/
cargo run -- upgrade [--apply]     # dry-run or apply storage compatibility actions
cargo run -- watch                 # foreground watch for the current repo
cargo run -- watch --daemon        # detached per-repo watch service
cargo run -- watch status          # watch ownership and reconcile telemetry
cargo run -- watch stop            # stop active watch service or clean stale watch artifacts
cargo run -- agent-setup <tool>    # write integration shim; automated tier (writes shim + MCP config): claude, codex, open-code, cursor, windsurf, roo; shim-only tier (writes shim only): copilot, generic, gemini, goose, kiro, qwen, junie, tabnine, trae; --regen to update if stale
cargo run -- setup                 # full onboarding: interactive TUI wizard (mode + agent target + explain)
cargo run -- setup <tool>          # full onboarding: scripted (init + shim + MCP register + first reconcile); --explain appends the explain sub-wizard
cargo run -- uninstall [--apply --force]  # guided full teardown; keeps .synrepo/ and ~/.synrepo data unless --delete-data is selected
cargo run -- search <query>        # lexical search
cargo run -- graph query "outbound <node_id> [edge_kind]"  # graph traversal
cargo run -- graph stats           # node/edge counts
cargo run -- node <node_id>        # dump a node's metadata as JSON
cargo run -- mcp                   # start MCP server over stdio
cargo run -- mcp --repo <path>     # start MCP server for a specific repo
RUST_LOG=debug cargo run -- <cmd>  # enable tracing output
openspec status --change <name> --json  # artifact/task completion check; isComplete=true when archivable
```

Dev dependencies: `proptest` (property tests for token budget invariants), `insta` (snapshot tests for card output), `tempfile` (test fixtures), `criterion` is available for explicit benchmark work (benchmarks only, not part of the documented workflow).

## Grep

Instead of grep or ripgrep use 'st' instead (syntext binary compatible with grep/rg).

## Hard invariants

These must hold across all changes:

1. `graph::Epistemic` has three variants: `ParserObserved`, `HumanDeclared`, `GitObserved`. Machine-authored content uses `overlay::OverlayEpistemic` instead. The type boundary is enforced by the type system — do not add machine variants to `Epistemic`.
2. The explain pipeline queries the graph with `source_store = "graph"` filtered at the retrieval layer. It never reads overlay output as input. This is structural, not just labeled.
3. `FileNodeId` is stable across renames within one discovery root. For new files it is derived from `(root_discriminant, first_seen_content_hash)` (`derive_file_id` in `pipeline/structural/ids.rs`). For existing files the stored ID is always reused. Content-hash rename detection (stage 6) is root-scoped: a file moved to a new path with identical content preserves its `FileNodeId` and records the old path in `path_history`. Do not derive `FileNodeId` from path.
4. `ConceptNodeId` is path-derived (`derive_concept_id` in `structure/prose.rs`), making it stable across content edits but not renames. This differs from `FileNodeId` — do not confuse the two.
5. `SymbolNodeId` is keyed on `(file_node_id, qualified_name, kind, body_hash)`. A body rewrite changes the hash but keeps the node's graph slot via upsert.
6. `EdgeKind::Governs` is only created from human-authored frontmatter or inline `# DECISION:` markers, never inferred.
7. `ConceptNode` is only created from human-authored markdown in configured directories (`docs/concepts/`, `docs/adr/`, `docs/decisions/` by default). The explain pipeline cannot mint concept nodes in any mode.
8. Any multi-query read through a `GraphStore` or `OverlayStore` must run under `with_graph_read_snapshot` / `with_overlay_read_snapshot` (or the trait's `begin_read_snapshot`/`end_read_snapshot` pair). Without a snapshot, a writer commit between queries leaves the reader observing two committed epochs in one operation, which is how cards end up citing nodes and edges from different generations.
9. Schema changes require `rm -rf .synrepo/ && synrepo init` against a fresh state. There are no migrations; `src/store/sqlite/schema.rs` and `src/store/overlay/schema.rs` ship one `CREATE TABLE` per table. `synrepo upgrade` rebuilds or invalidates per `CompatAction` but never migrates DDL. See `docs/SCHEMA.md` for the column-level reference.

## Gotchas

### File size and module structure

- **Every `.rs` file must stay under 400 lines** (test files count too). Split into a sub-module directory before exceeding the cap. To see the current over-cap set, run:

  ```bash
  find src -name '*.rs' -exec wc -l {} \; | awk '$1 > 400 {print $1, $2}' | sort -rn
  ```

  Persistent guidance for files that have repeatedly drifted over the cap:
  - `src/pipeline/repair/sync/handlers.rs` — the dispatch match is the body. Do not extend it in place; land new handlers in their own sibling files (see `revalidate_links.rs`).
  - `src/structure/parse/language.rs` — language registry; new languages should grow under `parse/extract/` and helpers, not the registry file.
  - `src/pipeline/explain/providers/*.rs` — request/response shaping is separable per provider.
  - `src/store/overlay/agent_notes.rs` and `src/store/overlay/commentary/mod.rs` — split read/write surfaces before adding fields.
  - `src/config/mod.rs` — defaults, parsing, and validation can each separate.

- **`src/tui/app/` sub-modules can own `impl AppState` blocks** for feature-specific state and handlers (see `explain_picker.rs` for the folder-picker modal). Keep `AppState` fields on the struct in `mod.rs`; put feature methods, helpers, and tests in the submodule.

### Agent shims and MCP

- **Agent-doctrine text lives in `src/surface/agent_doctrine.rs`** as the exported `doctrine_block!()` macro. `src/bin/cli_support/agent_shims/doctrine.rs` is only a binary-side re-export. Every shim under `agent_shims/shims/` (sub-module directory: `basic_targets.rs`, `markdown_targets.rs`, `skill_targets.rs`, `shared.rs`) embeds it via `concat!`. Edits touching escalation rules, do-not rules, fast-path hook signals, or the product-boundary paragraph MUST go through `doctrine_block!`; the byte-identical test (`every_shim_embeds_doctrine_block` in `agent_shims/tests/doctrine.rs`) enforces this. The escalation-line source-scan test reads `src/bin/cli_support/commands/mcp/tools.rs` (tool registrations); do not move the MCP tool registration out of that file without updating the test path. Edit target-specific sections (tool list framing, CLI fallback examples, file paths) directly in the relevant `shims/*.rs` file.
- **MCP read handlers must go through `SynrepoState::with_read_compiler` or `helpers::with_mcp_compiler`.** Those paths apply the per-repo snapshot concurrency limiter and the SQLite fallback compiler pool. Do not call `create_read_compiler()` directly from handlers or helper code; it bypasses MCP resource controls. `create_sqlite_compiler()` is reserved for explicit SQLite-backed mutation/fallback work such as commentary refresh.
- **MCP errors are structured.** Use `src/surface/mcp/error.rs::{McpError, ErrorCode}` plus `helpers::render_result`; do not hand-roll `{"error": "..."}`
  responses. Compatibility requires both `error.code` / `error.message` and the transitional flat `error_message` field. Map oversized inputs and bad enum values to `INVALID_PARAMETER`, missing graph targets to `NOT_FOUND`, initialization/prep failures to `NOT_INITIALIZED`, read-pressure failures to `BUSY`, and timeout wrappers to `TIMEOUT`.
- **MCP card/search helper layout is deliberate.** Batch card dispatch belongs in `src/surface/mcp/card_batch.rs`, single-card retry/rendering in `card_render.rs`, shared card-set token capping in `card_set.rs`, search cards mode in `search/cards_mode.rs`, enriched overview in `search/overview.rs`, and bidirectional impact in `search/impact.rs`. Do not grow `cards.rs`, `search.rs`, or `where_to_edit.rs` back toward the 400-line cap for these concerns.
- **MCP session state is binary-side.** Per-session defaults, rate-limit buckets, metrics, and tool-call timeout wrapping live under `src/bin/cli_support/commands/mcp/state.rs` and `state/session.rs`. `synrepo_use_project` sets a session default only after registry gating, and `synrepo://projects` is backed by `registry::load()`. Keep global/defaultless server convenience behind those registry checks.
- **Semantic MCP behavior must stay local-only and optional.** Query-time routing/search may use embeddings only when `semantic-triage` is compiled, config enables it, and local model/vector artifacts already load. Use `ModelResolver::resolve_existing` or `load_embedding_index`; do not trigger downloads from MCP handlers, hooks, or `task-route`. Deterministic safety guards in `task_route.rs` run before semantic intent matching, and missing assets must return `routing_strategy: "keyword_fallback"` or `semantic_available: false`.
- **Hybrid search result shape is mixed-source.** `synrepo_search mode=auto` fuses syntext top 100 and vector top 50 with RRF. Semantic-only rows may have `line`, `content`, and sometimes `path` as null until a graph lookup can enrich them. Keep `chunk_id`, `symbol_id`, `source`, `fusion_score`, and `semantic_score` stable so compact/cards modes can account for mixed lexical/semantic provenance.
- **Read-only MCP tools may degrade; mutating tools must not.** `synrepo_overview` can return partial probe/status data when repo prep fails, and path-like `synrepo_card` targets can return a degraded file stub. Edit and overlay-mutating tools should return structured initialization or lock errors instead of attempting partial degraded behavior.
- **`synrepo_apply_anchor_edits` is cross-file atomic.** Multi-file requests preflight every file before writing, write only after all validations pass, and restore prior originals if a later write fails. Tests should expect `atomicity.cross_file: true`, no partial write on preflight failure, and rollback status on write failure. Planning and rollback helpers live in `src/surface/mcp/edits/atomic.rs`; runtime/watch helpers live in `edits/runtime.rs`.
- **MCP surface changes need docs and source tests together.** New tools, resources, params, error behavior, rate limits, timeout behavior, or degradation semantics require updates to `docs/MCP.md`, `src/surface/mcp/README.md`, `skill/SKILL.md`, and the binary source-registration tests under `src/bin/cli_support/tests/mcp*.rs`.
- **Fast-path routing is advisory and read-only.** The shared classifier is `src/surface/task_route.rs`, with TypeScript parser proof in `src/surface/task_route/typescript.rs`. Expose new route intents through both `synrepo_task_route` and `synrepo task-route`, update hook signal rendering in `commands/agent_hooks/`, and keep source mutation behind `synrepo_apply_anchor_edits`.
- **Agent install ownership goes through `agent-config`.** MCP, skill, and instruction installs use `agent-config = "0.1"` with owner/name `synrepo`; remove must call the matching uninstall API before any path-based fallback. `synrepo upgrade --apply` adopts legacy unowned installs into the ledger. `AgentTool::output_path()` is now a local-scope status probe with a legacy fallback; keep new path logic in the installer/status layer rather than adding another hard-coded table.

### Links, repair, explain

- **`src/bin/cli_support/commands/links/accept.rs` owns the curated `links accept` 3-phase commit path** and the debug-only crash failpoints used by the soak suite. Keep `SYNREPO_TEST_CRASH_AT=links_accept:after_pending` and `links_accept:after_graph_insert` test-only, and prefer extending the submodule instead of growing `commands/links.rs` again.
- **`repair/types/` has dual string mappings**: `RepairSurface`, `DriftClass`, `Severity`, and `RepairAction` each have `#[serde(rename_all = "snake_case")]` AND a manual `as_str()` in `src/pipeline/repair/types/stable.rs`. Adding a new variant requires updating both. `RepairSurface::ProposedLinksOverlay`, `RepairSurface::ExportSurface`, `RepairAction::RevalidateLinks`, and `RepairAction::RegenerateExports` follow the same rule. The stable-identifier tests in `src/pipeline/repair/types/tests.rs` catch `as_str()` divergence from literals but do not cross-check serde output.
- **Commentary freshness is computed in two places**: the status command surfaces it from `src/bin/cli_support/commands/status/text.rs` and `status/json.rs` (the `status` command is a sub-module directory, not a single file), and the repair surface computes it in `src/pipeline/repair/report/surfaces/commentary.rs::scan_commentary_staleness`. Both walk `commentary_hashes()` against `resolve_commentary_node`. The repair version is `pub(super) struct CommentaryScan { total, stale }`; unifying requires promoting it to `pub` in the repair module.
- **Explain docs are advisory overlay output only.** Materialized commentary docs live under `.synrepo/explain-docs/` with a dedicated syntext index at `.synrepo/explain-index/`. They are searchable through `synrepo_docs_search`, but they are never canonical graph facts and never used as explain input.
- **`RepairAction::RevalidateLinks` runs the fuzzy-LCS verifier (`cross_link_verify::verify_candidate_payload`) and, on success, updates the candidate's stored hashes and spans in place via `OverlayStore::revalidate_link`.** Mismatches stay in `report_only` — never auto-rejected; rejection is a human call. Handler implementation lives in `src/pipeline/repair/sync/revalidate_links.rs`. `sync/handlers.rs` is over the 400-line cap; land any new repair-action handler in its own sibling file, do not grow the dispatch match in place.

### Graph semantics

- **`signature` and `doc_comment` are populated** for Rust (`///` line comments, declaration up to `{`/`;`), Python (docstring, `def` line up to `:`), and TypeScript/TSX (JSDoc `/** */`, declaration up to `{`). Go gets `None` for both (not yet wired — see `docs/ADDING-LANGUAGE.md` step 5).
- **`all_edges()` excludes retired edges.** Use `all_edges()` for drift scoring and card compilation (both only care about active edges). For retired edges (e.g. compaction enumeration), query the `edges` table directly without the `retired_at_rev IS NULL` filter.
- **Prefer `GraphStore` bulk list APIs over `all_X_names` + per-row `get_X`.** `all_symbols_summary` (id/file/qname/kind/body_hash) and `all_symbols_for_resolution` (id/file/qname/kind/visibility) return pre-joined tuples in one SELECT; the docstrings call the paired pattern an N+1 anti-pattern. `kind` and `body_hash` are dedicated columns, but `visibility` lives in the `data` JSON blob, so bulk queries needing it deserialize a thin serde slice.
- **FileNodeId is stable across content edits.** Content-hash changes no longer delete and re-insert file nodes. Instead, `content_hash` is a version field updated in place, and `last_observed_rev` advances. The identity domain is root-scoped by `FileNode.root_id`, so identical bytes in the primary checkout and a linked worktree produce distinct `FileNodeId`s. Symbols and edges owned by the file are retired (marked `retired_at_rev`) if not re-emitted in the new compile, or re-activated if they re-appear. This enables drift scoring across observation windows.
- **Edited rename detection is structural first and bounded second.** Same-root high symbol-set overlap can preserve a disappeared file's ID. Symbol-poor files may compare `FileNode.content_sample_hashes`, but only under the hard sample file-size cap in `structure/identity/detect.rs`. Do not add full byte LCS or cross-root matching to the structural compile hot path. Any direct `FileNode` fixture must include `content_sample_hashes`, usually `Vec::new()` for synthetic rows.
- **Retired observations are soft-deleted until compaction.** Symbols and edges with `retired_at_rev` set remain in the store but are excluded from `active_edges()` and card compilation. Physical deletion only occurs via `compact_retired(older_than_rev)` which runs during `synrepo sync` and `synrepo upgrade --apply`. The `retain_retired_revisions` config (default 10) controls how many revisions of retired observations to keep before compaction.

### Stores, transactions, snapshots

- **`.synrepo/graph/nodes.db`** is the actual SQLite file. Code that opens the graph store uses `SqliteGraphStore::open(&graph_dir)` where `graph_dir` is `.synrepo/graph/`; the `nodes.db` name is internal to `src/store/sqlite/mod.rs`.
- **Compatibility blocks on version mismatch**: if `.synrepo/` contains a graph store whose recorded format version is newer than the current binary understands, `synrepo init` and all graph commands will error. Run `synrepo upgrade` to see recovery steps; for a full reset, remove `.synrepo/` and run `synrepo init`.
- **Structural compile is a single atomic transaction (stages 1–4)**: `run_structural_compile` wraps all four stages in one `BEGIN`/`COMMIT`. Stage 4 reads uncommitted nodes from stages 1–3 via SQLite read-your-own-writes on the same connection. The `with_transaction` helper that existed in `structural/mod.rs` has been removed; do not re-add it.
- **Reader snapshots are re-entrant**: `SqliteGraphStore::begin_read_snapshot` and the overlay equivalent use a `Mutex<usize>` depth counter. Only the outermost begin issues `BEGIN DEFERRED`; only the outermost end issues `COMMIT`. This lets an MCP handler wrap a request while `GraphCardCompiler` also wraps each method internally without tripping SQLite's "transaction within a transaction" error. Writer-side `begin`/`commit`/`rollback` is a separate lane (`&mut self`) and must not interleave with a read snapshot on the same handle. Note: `BEGIN DEFERRED` only upgrades to a real read transaction on the first SELECT, so the snapshot epoch is pinned at the first read, not at begin.
- **Both SQLite stores set `busy_timeout = 5000`** (see `src/store/sqlite/schema.rs` and `src/store/overlay/schema.rs`) so transient WAL checkpoint contention waits up to 5 s rather than surfacing `SQLITE_BUSY`. This becomes load-bearing when readers hold snapshots across writer commits.
- **Persisted state-struct fields need `#[serde(alias = "old_name")]` on rename.** `WatchDaemonState`, `WriterOwnership`, and reconcile-state structs are serialized to `.synrepo/state/*.json` by live daemons — renaming a `pub` field without the alias leaves existing JSON files unparseable after an upgrade.

### Git and watch

- **Git history mining uses `gix`** (not `git2`). The history collector in `pipeline/git/mod.rs` disables rewrite tracking for performance. The rename fallback in `pipeline/git/renames.rs` enables it separately for the identity cascade step 4. All `gix` usage is centralized under `src/pipeline/git/`.
- **`notify` and `notify-debouncer-full` are in `Cargo.toml`** and power the shipped watch runtime in `src/pipeline/watch/service.rs`. The service runs both `synrepo watch` foreground mode and `synrepo watch --daemon`, with `.synrepo/` self-event suppression, startup reconcile before steady-state watching, and watch-time incremental lexical index maintenance when a trustworthy touched-path batch exists.
- **Discovery is multi-root.** The primary checkout is always a root. Linked worktrees are included by default (`include_worktrees = true`), initialized submodules are opt-in (`include_submodules = false`), and each root has its own `FileNode.root_id`. Same relative paths in two roots are distinct graph files; cross-root `Calls`, `Imports`, and `CoChangesWith` edges are not emitted.
- **Watch is root-scoped.** The watch service registers every discovery root and records absolute touched paths. Reconcile ownership uses longest-prefix matching over discovery roots, so a write in worktree A scopes structural deletion/rename handling to worktree A and does not retire files from worktree B.
- **Watch control plane is `interprocess::local_socket`** (Unix domain socket on Unix, named pipe on Windows). Use `watch_control_endpoint(synrepo_dir)` to get the platform-appropriate endpoint string and `watch_control_socket_name(endpoint)` to build the `interprocess::local_socket::Name` — do not concatenate `.sock` paths by hand. The Windows endpoint is `synrepo-watch-<blake3-hash-prefix>` and has no on-disk artifact; `watch_socket_path` remains Unix-only for cleanup of the stale socket file. Live dashboard mode streams `WatchEvent`s from the service to the TUI via a `crossbeam_channel` bounded to 256 — sends are best-effort (`let _ = tx.send(...)`); a dropped receiver must not kill the reconcile loop.
- **TUI watch lifecycle is centralized in `src/tui/watcher.rs::WatcherSupervisor`.** `AppState` delegates mode via `watcher_mode()`; do not reintroduce a parallel `AppState.watcher_mode` field.
- **Watch lease and writer lock are separate**: `.synrepo/state/watch-daemon.json` records long-lived watch ownership, while `.synrepo/state/writer.lock` still guards each actual write. `synrepo reconcile` delegates to the watch owner when watch is active; unsupported mutating commands fail fast until watch is stopped. Remove stale watch or writer artifacts only after confirming the recorded PID is dead (`kill -0 <pid>` returns non-zero). The canonical structural write path remains `run_reconcile_pass()` in `src/pipeline/watch/reconcile.rs`.

### Writer lock (`.synrepo/state/writer.lock`)

- **It is a kernel advisory lock (`flock`/`LockFileEx` via `fs2`), not file existence.** Tests that only stamp JSON into the lock file do NOT exercise contention.
- **Simulating contention:** `synrepo::pipeline::writer::hold_writer_flock_with_ownership(lock_path, &WriterOwnership { pid, acquired_at })` takes the flock on a separate fd. Dead-PID / live-foreign-PID helpers: `writer::spawn_and_reap_pid()` and `writer::live_foreign_pid() -> (Child, u32)`. Do not re-implement these.
- **Acquire retries `WouldBlock` briefly** via `open_and_try_lock_with_retry` in `src/pipeline/writer/helpers.rs` (20 × 5 ms). macOS can delay flock release propagation across fds, so a plain `try_lock_exclusive` returns `WouldBlock` even when no live holder exists. Do not flatten this back to `open_and_try_lock` — it reintroduces the mutation-test flake.
- **Windows contention detection:** `fs2::try_lock_exclusive` on Windows surfaces raw `ERROR_LOCK_VIOLATION` (os error 33), which Rust std does NOT map to `ErrorKind::WouldBlock`. Use `pipeline::writer::is_lock_contention(&err)` for cross-platform contention checks; it accepts both `WouldBlock` and raw os 33.
- `libc` is a `[target.'cfg(unix)'.dependencies]` dep used only for `O_CLOEXEC` on the lock fd.

### Testing and CI

- **`cargo build --workspace` does not imply `cargo test` compiles**: test-scoped code (`#[cfg(test)]` and `mod tests`) only compiles under `cargo test` / `cargo check --tests` / `cargo clippy --all-targets`. A pre-existing test-only compile error in an unrelated module will surface there, not in `cargo build`.
- **`cargo test --workspace` is not trusted in parallel for CI**: some mutation-surface tests interfere under parallel execution even when they pass in isolation and under `--test-threads=1`. CI uses `make ci-test` (serial workspace tests); use `make check` locally for the faster parallel pass, and confirm suspicious failures with a focused rerun or `make ci-test`.
- **CI lint excludes test targets**: `make ci-lint` and the GitHub Actions workflow run `cargo clippy --workspace --bins --lib -- -D warnings`. `--all-targets` currently trips a backlog of unrelated test-only lints, so do not treat `make check` as a publish gate until that debt is burned down.
- **`tests/mutation_soak.rs` is an ignored Unix-only release-gate suite** covering `links accept` crash recovery, watch-active mutation blocking (`export`, `compact --apply`, `upgrade --apply`), abrupt watch-daemon death cleanup, and repeated writer-lock contention under real subprocess load. Run it serially with `cargo test --test mutation_soak -- --ignored --test-threads=1` or `make soak-test`; keep it out of default CI unless the workflow is intentionally being expanded.
- **Binary crate test visibility**: In `src/bin/cli_support/tests/`, functions are accessible as `crate::<name>` only if imported at the binary root (`cli.rs`) via `use`. Modules declared `mod <name>` inside `commands/` are private — tests cannot reference them by full path; use the re-exported name instead.
- **Lib-internal test helpers needed by *both* lib tests and bin-crate tests must be `pub #[doc(hidden)]`**, not `pub(crate)` or `#[cfg(test)]`. Bin-crate tests compile the library *without* `cfg(test)`, so `#[cfg(test)]` items and `pub(crate)` are invisible to them. Canonical example: the writer module's `hold_writer_flock_with_ownership` / `spawn_and_reap_pid` / `live_foreign_pid` / `TestFlockHolder` in `src/pipeline/writer/helpers.rs`, re-exported from `src/pipeline/writer/mod.rs`.
- **`src/test_support.rs` holds the hidden cross-process test lock helper** used to serialize a few mutation-heavy tests across both lib and bin test binaries. Keep it test-only and use it sparingly for real contention flakes, not as a substitute for fixing product code.
- **Test fixtures that create multiple files in the same discovery root must not share byte-identical content.** `FileNodeId` is root + content-hashed for new files (see invariant 3), so two same-root files with the same bytes collapse to the same node and one overwrites the other. Identical bytes across different roots are valid and should produce distinct IDs. Differentiate same-root files with a leading comment or distinct body when a test needs multiple files.
- **`make_ready_poll_state()` in `src/tui/app/tests.rs`** returns `(TempDir, AppState)` after running `bootstrap()` into the tempdir. Use it for TUI tests that exercise `Config::load` or any other on-disk read from `AppState`; the plain `make_poll_state()` skips bootstrap and will fail those paths.

### Windows

- **Host `cargo clippy` does not compile `#[cfg(windows)]` blocks**, so Windows-only lints (e.g. `clippy::needless_return`) only fire on the Windows CI runner. When adding or touching `cfg(windows)` code, run `cargo clippy --target x86_64-pc-windows-gnu --bins --lib -- -D warnings` locally before pushing.
- **Windows CI runs `make ci-test` before `make ci-lint`** and stops on first failure. A lib-crate test failure masks bin-crate test failures and downstream lint failures in the same run; assume there may be additional failures when fixing a Windows-only lib test.

### Misc

- **`bootstrap()` signature**: `synrepo::bootstrap::bootstrap(repo_root: &Path, mode: Option<Mode>)` — two args only. Does not accept a pre-built `Config` or `synrepo_dir`; it derives both internally.
- **`openspec/changes/archive/` is historical.** Do not edit archived proposals/specs/tasks when updating path references or API shapes — only living specs under `openspec/specs/` and runtime code.
- **Adding a new `Language` variant is surface-enforced.** See `docs/ADDING-LANGUAGE.md`. Tests fail loud if any required surface is missed.

## OpenSpec workflow

`openspec/specs/` holds enduring domain specs (stable intended behavior). `openspec/changes/<name>/` holds active work: `proposal.md`, `design.md`, `tasks.md`, and optional delta specs.

Active changes live in `openspec/changes/<name>/`; archived work moves to `openspec/changes/archive/`. Check the directory listing for the current set rather than this file.

Specs govern intent; the graph governs runtime truth.
