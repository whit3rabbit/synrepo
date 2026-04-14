## Context

Stage 5 of the structural pipeline mines per-file first-parent commit history, hotspots, ownership hints, and co-change partners through `pipeline::git_intelligence::analyze_path_history` (see `src/pipeline/git_intelligence/analysis.rs:88`). The projection from the raw mining output to a card-ready payload — `FileGitIntelligence::from(GitPathHistoryInsights)` — already exists at `src/surface/card/git.rs:68`. Neither is called by the card compiler: `src/surface/card/compiler/file.rs:81` sets `git_intelligence: None` unconditionally, and `src/surface/card/compiler/symbol.rs:69` sets `last_change: None`. `SymbolCard.last_change` is declared as `Option<String>` in `src/surface/card/types.rs:51` with no populator.

The `git-intelligence-v1` change archived the mining half of this work but not the surfacing half. ROADMAP.md §11.1 records this as "Track D / I — data computed but not surfaced"; §11.3 captures the design for how `SymbolCard.last_change` should look.

`GraphCardCompiler::new` already accepts a `repo_root: Option<PathBuf>`, and `GitIntelligenceContext` is constructed from a repo root, so the compiler holds everything needed to open a context. Compiler methods take `&self` and wrap reads in `with_graph_read_snapshot`, so any caching state needs interior mutability.

Card compiler snapshot tests under `src/surface/card/compiler/snapshots/*.snap` pin the JSON shape of `SymbolCard.last_change`; changing the shape requires regenerating them.

Stakeholders: the CLI (`synrepo_card`, `synrepo export`), the MCP server (`synrepo-mcp` crate), and any downstream agent consuming cards.

## Goals / Non-Goals

**Goals:**
- `FileCard.git_intelligence` is populated at Normal and Deep budget when a `GitIntelligenceContext` can be opened for the compile session, and carries degraded-history status when it cannot.
- `SymbolCard.last_change` surfaces "when and by whom was this changed" with an explicit granularity label, so downstream consumers can discount file-level approximations and transparently upgrade to symbol-level precision when it lands.
- The card compiler stays cheap to call repeatedly within one reconcile epoch: opening a git context and walking history happens at most once per compiler instance and at most once per file path.
- The stale `src/surface/card/compiler/mod.rs` doc block that still claims `FileCard.git_intelligence` is `None` pending `git-intelligence-v1` is corrected (the change already archived).

**Non-Goals:**
- Emitting graph-level `CoChangesWith` edges. The edge kind exists but no writer produces it; that work is deferred until the file-card payload has stabilized under real traffic.
- Symbol-level granularity for `last_change`. That requires new `SymbolNode` fields, a one-time backfill pass, and per-reconcile update logic (ROADMAP §11.3 Option D). Out of scope here.
- Extending `synrepo_card` to accept directory targets. Tracked separately.
- Whitespace-only / format-only commit filtering. Content-weighted ranking is a Track I enhancement, not a prerequisite for surfacing `last_change`.
- Blame-based or diff-scan approaches to `last_change` (ROADMAP §11.3 Options B/C). They break across renames; the illusion of precision is worse than explicit approximation.

## Decisions

### D1. Replace `SymbolCard.last_change: Option<String>` with a structured type.

New types in `src/surface/card/types.rs`:

```rust
pub enum LastChangeGranularity { File, Symbol, Unknown }

pub struct SymbolLastChange {
    pub revision: String,                      // hex SHA, short form at normal, full at deep
    pub author_name: String,
    pub committed_at_unix: i64,
    pub granularity: LastChangeGranularity,
    pub summary: Option<String>,               // populated only at deep
}
```

**Why over alternatives:**
- A bare `Option<String>` (the status quo) cannot carry the granularity label that invariant 4 ("smallest truthful context first") requires. An agent that cannot distinguish file-granularity from symbol-granularity approximations makes worse routing decisions than one that can.
- A blame- or diff-scan-based populator of the current `String` field would look more precise but silently reset across file renames. The explicit `granularity` enum makes the current v1 approximation visible without locking out the Option D symbol-level upgrade.
- Keeping the string type but changing its semantics ("sometimes file-level, sometimes symbol-level, you figure it out") is the worst option: consumers have no stable contract.

**BREAKING**: downstream JSON consumers see `null` → object transition. Pre-1.0; acceptable. No deprecation window.

### D2. Cache the `GitIntelligenceContext` and per-path analysis on `GraphCardCompiler`.

`GraphCardCompiler` gains a single cache field:

```rust
git_cache: parking_lot::Mutex<GitCacheState>,

enum GitCacheState {
    Uninitialized,
    Unavailable,                                // open failed; stop retrying
    Ready {
        context: GitIntelligenceContext,
        by_path: HashMap<String, Arc<FileGitIntelligence>>,
    },
}
```

**Why:**
- Compiler methods take `&self`; `parking_lot::Mutex` is already the conventional choice in the codebase (overlay store, other caches).
- `Arc<FileGitIntelligence>` so the cached entry can be cloned cheaply into both a `FileCard` and the projection used for a `SymbolCard.last_change` without re-walking history.
- The MCP server constructs a fresh compiler per reconcile epoch, so compiler-lifetime caching is the correct invalidation scope. No explicit invalidation logic needed.
- `Unavailable` is cached explicitly so a `git open` failure on the first call does not re-attempt on every subsequent card request.

**Alternatives rejected:**
- Per-call memoization (cache lives inside a single `file_card` / `symbol_card` call): does not help the common MCP traffic pattern of N card requests for related files.
- External cache keyed by `(repo_revision, path)` in `.synrepo/state/`: adds a storage contract and a new invalidation story. Compiler-lifetime caching is sufficient for the observed access pattern.
- Precomputing all paths up front during compiler construction: wasted work for compilers that answer one card request.

### D3. `git_commit_depth` from `Config` drives `max_commits`; a small constant caps `max_results`.

- `max_commits`: pass through `Config.git_commit_depth` (default 500). This is the deterministic sampling budget the user already tunes for git intelligence.
- `max_results`: compile-time constant of 8 for `commits` and `co_change_partners` in the card payload. Cards are token-bounded; more than a handful of entries is noise.

This keeps one user-visible knob and avoids inventing a new card-scoped config field.

### D4. `SymbolCard.last_change` projection: derive from the cached `FileGitIntelligence`.

Populated for `Normal` and `Deep`, absent for `Tiny`:
- `revision`: `commits[0].revision` (newest first-parent touch of the containing file)
- `author_name`, `committed_at_unix`: from the same commit
- `summary`: populated only for `Deep`; omitted for `Normal`
- `granularity`: `File` if commits non-empty; `Unknown` if readiness is not `Ready` or no commits present

A helper in `src/surface/card/git.rs` — `SymbolLastChange::from_file_intelligence(&FileGitIntelligence, Budget)` — returns `None` when there is no first commit or status is not `Ready` with data.

### D5. `FileCard.git_intelligence` population scheme.

- `Tiny`: `None` (unchanged).
- `Normal` and `Deep`: populate via cache lookup; on `Unavailable`, leave `None` (no synthetic degraded payload at the card layer — the cache was explicitly "git not available here").
- On `Ready` context but degraded history for a specific path (empty commits), the payload carries `status: Degraded` (or whatever readiness enum value applies) plus empty sub-fields. Never silently elided.

### D6. Rewrite the stale doc comment at `src/surface/card/compiler/mod.rs:8-9`.

Small, in-scope: the comment still points at the archived `git-intelligence-v1` as the gating change. Replace with a comment explaining current behavior after this change.

## Risks / Trade-offs

- [First-card latency spike on a cold compiler]: opening a `GitIntelligenceContext` and walking up to 500 commits happens on the first Normal+/Deep card request. → Amortized across subsequent requests via the cache; optionally tracked via existing tracing if it becomes a hot path.
- [`SymbolCard` JSON break]: pre-1.0 contract change. Any serialized fixture or external tool relying on `last_change: null | string` will fail. → Snapshot tests catch our own fixtures; we call the break out in the proposal; no deprecation window.
- [File-rename ambiguity]: `analyze_path_history` is called with the current `FileNode.path`. If the file was renamed recently, history beyond the rename is not surfaced (first-parent follow across renames is not enabled). → Acceptable under the `granularity: File` label; users who need cross-rename history get it via Option D later.
- [Cache unbounded by path count]: every distinct file path queried during one compiler lifetime stays in memory. → Compiler lifetime is one reconcile epoch; MCP rebuilds it. If this becomes a memory issue, bound the map with an LRU later without contract impact.
- [Degraded-history cards carrying empty payloads might mislead agents]: a `FileGitIntelligence` with empty `commits` and `status: Degraded` could look like "no churn" rather than "no data". → The `status` field carries the readiness signal; downstream consumers must branch on status, not presence of commits. Documented in the cards spec delta.
- [Git open failure at compiler construction is invisible]: we don't log every cache miss that turns into `Unavailable`. → Log once at cache-transition-to-Unavailable via `tracing::warn!` to make the reason discoverable.

## Migration Plan

1. Add new types to `src/surface/card/types.rs`; switch `SymbolCard.last_change` to the new optional struct.
2. Add `git_cache` field and helpers to `GraphCardCompiler`.
3. Wire `file_card` and `symbol_card` populators.
4. Regenerate snapshot tests via `cargo insta test --review` after confirming the diff matches the new contract.
5. Update the ROADMAP.md §11.1 bullets on archive (not during implementation — archive-time cleanup only, per `openspec-archive-change` norms).

No runtime data migration. No storage layout change. No new config field. Rollback is a revert.

## Open Questions

1. **`max_results` cap value**: the design picks 8 without data. Existing file-card output in `pipeline/git_intelligence` surfaces per-path summaries; the card-layer cap may want to track that existing limit. Verify during implementation that 8 matches or undercuts the mining-layer default; adjust if the mining-layer default is smaller.
2. **`revision` short-form at Normal**: §11.3 says "short SHA" at Normal without specifying length. Default to the first 7 hex chars unless a repo convention is found; open for review during implementation.
3. **Should `FileCard.git_intelligence` respect a new budget-level truncation (e.g. `commits.len() == 3` at Normal, `== 8` at Deep)?** The proposal does not commit to one; deferring to implementation. The default will be the same `max_results` at both tiers with `Deep` adding the full `source_body` on symbols as the only delta; revisit if token-budget pressure shows up in snapshots.
