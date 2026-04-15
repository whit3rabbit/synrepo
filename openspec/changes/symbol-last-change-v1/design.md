## Context

`SymbolCard.last_change` currently reports the most recent first-parent commit that touched the symbol's containing file, labeled `granularity: "file"`. This is the v1 approximation shipped in `git-data-surfacing-v1`. The upgrade path to symbol-level tracking was deliberately left as an explicit TODO in both the git-intelligence and cards specs.

The prerequisite machinery is already in place:
- `SymbolNode.body_hash` is a blake3 hash of the symbol's body bytes, recomputed on every structural compile.
- `SymbolNodeId` is keyed on `(file_id, qualified_name, kind, body_hash)`, so a body rewrite already produces a new identity revision (upsert semantics in the graph store).
- Stage 5 (`src/pipeline/git/`) walks sampled first-parent history and produces `GitPathHistoryInsights` per file.
- `GitCache` (`src/surface/card/git_cache/`) provides per-compiler caching with HEAD invalidation.
- `symbol_last_change_from_insights` (`src/surface/card/git.rs`) currently projects the file's newest commit.

The gap: nobody diff's the `body_hash` across sampled commits to find which commit actually changed a specific symbol.

## Goals / Non-Goals

**Goals:**
- Derive per-symbol `first_seen_rev` and `last_modified_rev` from `body_hash` transitions in sampled history.
- Surface `granularity: "symbol"` on `SymbolCard.last_change` when symbol-scoped data is available, with `"file"` as the documented fallback.
- Stay within the existing `git_commit_depth` budget; no additional history walks.

**Non-Goals:**
- Per-symbol blame or line-level attribution (too expensive; out of scope).
- `CoChangesWith` graph edges between symbols (separate roadmap item).
- Detecting which specific symbols changed in historical commits for files not in the current tree (we only parse the current file state).

## Decisions

### D1: Store revisions on SymbolNode, not in a side table

Add `first_seen_rev: Option<String>` and `last_modified_rev: Option<String>` columns to the `SymbolNode` row in `nodes.db`.

**Why over alternatives:**
- A side table adds join complexity and another migration path for no operational benefit. The fields are intrinsic to the symbol's identity in the same way `body_hash` is.
- Keeping them on `SymbolNode` means the card compiler reads them in the same query that fetches the symbol, at zero additional cost.

**Tradeoff:** Every `SymbolNode` row grows by two nullable `TEXT` columns. For a 10k-symbol repo, this is negligible. The upsert path must write these fields alongside the existing columns.

### D2: Derive revisions from body_hash diffing in stage 5

Stage 5 already walks sampled commits per file. The extension: for each file's sampled commit sequence, extract the symbol tree at each sampled revision (parse the file at that revision's content), and compare `body_hash` values for symbols whose qualified names match across adjacent commits.

**Mechanism:**
1. Stage 5 already has the file's sampled commit list and the current `body_hash` map from the structural compile (stages 1-3).
2. For each sampled commit (walking newest-to-oldest), extract the file content at that revision via `gix`, parse it through the same tree-sitter pipeline to get `body_hash` per qualified name.
3. For each symbol present in the current compile, walk the sampled commits. The first commit where the `body_hash` differs from the current value establishes `last_modified_rev`. If no hash transition is found in the sampled window, `last_modified_rev` falls back to the file-level newest commit.
4. `first_seen_rev` is the oldest sampled commit where the symbol's qualified name appears. This is a lower bound (the symbol may be older than the sampling window).

**Why over alternatives:**
- `git blame` would give line-level precision but is too slow for every symbol in every file during a full compile, and it doesn't map cleanly to `body_hash` semantics.
- Storing a diff of symbol trees at each commit would work but is more complex than re-parsing at the sampled revisions.
- Only re-parsing at sampled commits (within `git_commit_depth`) keeps the cost bounded and proportional to the configured budget.

### D3: Fallback to file granularity when symbol-scoped data is unavailable

When:
- The symbol was not present in any historical parse (new symbol), or
- No `body_hash` transition was found in the sampled window, or
- Git history is degraded,

the `last_change` payload uses the file-level projection with `granularity: "file"`, exactly as today. No `granularity: "unknown"` for this case; `unknown` remains reserved for degraded git history.

**Why:** The file-level fallback is already accurate (the file was touched in that commit). Downgrading to `unknown` would lose information. The spec's existing non-breaking upgrade promise means consumers already handle both `"file"` and `"symbol"` values.

### D4: Card compiler reads revisions from SymbolNode

`symbol_last_change_from_insights` gains access to the `SymbolNode`'s stored revisions. If `last_modified_rev` is `Some(rev)`, it resolves that revision's metadata (author, timestamp, summary) from the git insights and returns `granularity: "symbol"`. Otherwise it falls back to the current file-level behavior.

**Why over alternatives:**
- Resolving the revision metadata from the cached `GitPathHistoryInsights` avoids an additional git walk during card compilation. The sampled history already carries per-commit metadata.
- If the revision is not in the sampled window (symbol changed before the window), the file-level fallback applies, which is correct.

## Risks / Trade-offs

- **[Performance]** Parsing each file at every sampled commit adds compile time proportional to `git_commit_depth` times the number of files with symbols. Mitigation: only parse files that have changed in the sampled window (stage 5 already knows which files appear in which commits). For files with no sampled touches, no additional parsing occurs.
- **[Accuracy]** `first_seen_rev` is a lower bound, not exact. The symbol may predate the sampling window. Mitigation: the field is labeled as `git_observed` with `granularity: "symbol"`; consumers already know git signals are approximate. The spec can note this explicitly.
- **[Symbol rename]** If a symbol is renamed (qualified name changes) but its body is unchanged, `body_hash` matching across the rename is possible but qualified-name matching is not. This produces a false negative: the renamed symbol looks "new" with no `last_modified_rev`. Mitigation: acceptable in v1. The file-level fallback still provides a useful signal. Identity cascade (stage 6) will eventually handle this.
- **[Schema migration]** Additive columns only. `synrepo upgrade --apply` adds `first_seen_rev TEXT NULL` and `last_modified_rev TEXT NULL` to the `symbols` table. Existing rows get `NULL` until the next compile populates them. No data loss, no rebuild required.
