## Why

Stage 5 already computes per-file git history, hotspots, ownership, and co-change partners via `pipeline::git_intelligence::analyze_path_history`, and `surface::card::git::FileGitIntelligence::from(...)` already converts that output to the card payload shape. The card compiler never invokes either. `FileCard.git_intelligence` is unconditionally `None` at `src/surface/card/compiler/file.rs:81`, and `SymbolCard.last_change` is an unused `Option<String>` stub. The `git-intelligence-v1` change archived the mining pipeline without delivering the card-surface half of the commitment, so agents cannot route on recency, ownership, or co-change even though the data is sitting one call away.

## What Changes

- Wire `FileCard.git_intelligence` at `Normal` and `Deep` budgets by calling `analyze_path_history` from the file-card compiler. Left `None` at `Tiny`.
- Cache the opened `GitIntelligenceContext` and per-path analysis results on `GraphCardCompiler` for the lifetime of the compiler instance, so repeated card requests in one reconcile epoch do not re-walk history.
- **BREAKING**: replace `SymbolCard.last_change: Option<String>` with `Option<SymbolLastChange>` plus a `LastChangeGranularity` enum (`File` | `Symbol` | `Unknown`), per the design captured in ROADMAP §11.3. V1 emits only `File` and `Unknown`; `Symbol` is reserved for the later graph-native body-hash tracking upgrade.
- Populate `SymbolCard.last_change` at `Normal` (SHA, author, committed-at, granularity) and `Deep` (above plus folded one-line summary). Absent at `Tiny`.
- When git history is absent or degraded (`GitIntelligenceReadiness` not `Ready`), `FileCard.git_intelligence` carries the status with empty sub-fields, and `SymbolCard.last_change` uses `granularity: Unknown`. Never elided silently.
- Rewrite the stale `src/surface/card/compiler/mod.rs` doc comment that still claims `FileCard.git_intelligence` is `None` pending `git-intelligence-v1`.
- Update the `cards` spec to define the new `last_change` shape and to require `FileCard.git_intelligence` population at Normal+Deep when history is available.
- Update the `git-intelligence` spec to add the card-surfacing contract for file-level history and the `File`-granularity `last_change` projection.

Graph-level `CoChangesWith` edge emission is explicitly **not** in scope — it is deferred until the per-file payload shape has stabilized under real card traffic. The `synrepo_card` directory-target extension, the `.synrepo/cache/` placeholder cleanup, and the `entry_point.rs` test split are also not in scope.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cards`: `SymbolCard.last_change` gains a structured shape with explicit granularity labeling; `FileCard.git_intelligence` must be populated at Normal+Deep when the git intelligence context is available.
- `git-intelligence`: adds the file-card surfacing contract (per-path history, ownership, co-change partners) and the `File`-granularity `last_change` projection contract. Caching behavior (per-compiler-lifetime) is a non-normative implementation note.

## Impact

- **Code**:
  - `src/surface/card/types.rs` — new `SymbolLastChange` struct, new `LastChangeGranularity` enum, changed `SymbolCard.last_change` field type.
  - `src/surface/card/compiler/mod.rs` — `GraphCardCompiler` gains a cache handle (`parking_lot::Mutex<Option<GitIntelligenceCache>>`); stale doc comment rewritten.
  - `src/surface/card/compiler/file.rs` — reads the cache and populates `git_intelligence` for Normal+Deep.
  - `src/surface/card/compiler/symbol.rs` — reads the cache and populates `last_change` for Normal+Deep; passes the repo path of the symbol's containing file.
  - `src/surface/card/git.rs` — may gain a helper that projects a `FileGitIntelligence` to a `SymbolLastChange` (file-granularity).
  - Card compiler snapshot tests (`src/surface/card/compiler/snapshots/*.snap`) regenerated for the new `last_change` shape and new `git_intelligence` payload.
- **Public surface**: `SymbolCard` JSON shape changes (pre-1.0; acceptable). Every external consumer reading `last_change` sees an object instead of a string.
- **Dependencies**: none added.
- **Performance**: opening `GitIntelligenceContext` is one-time per compiler instance; `analyze_path_history` is memoized per file path within that lifetime.
- **Runtime state**: none; cache is in-memory only.
- **Specs**: deltas to `openspec/specs/cards/spec.md` and `openspec/specs/git-intelligence/spec.md`.
- **ROADMAP.md**: on archive, move the relevant §11.1 bullets from "Track D / I — data computed but not surfaced" to "Track E — compiled and wired".
