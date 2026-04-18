## Why

`best_fuzzy_match` at `src/pipeline/repair/cross_link_verify.rs:171-209` refuses to run on any input where `original_source.len() > 4096` or `needle.len() > 256`, returning `None` silently. Phase 1 inspection confirmed:

- The 4KB cap is deliberate protection against the algorithm's cost: for source length N and needle length M, the nested loops are approximately `O(N * M²)` (per-word-window LCS, word-count windows, O(M²) DP each). At N=4096, M=256, that is ≈ 268M byte-comparisons per call, already at the edge of tolerable.
- The algorithm does a final "is needle an exact substring of normalized source?" check at line 157 **after** the fuzzy windowed LCS runs. When exact-substring match succeeds, the fuzzy work was wasted.
- Symbol bodies and prose docs routinely exceed 4KB (any reasonably-sized function or ADR). Cross-link verification silently degrades or drops these spans without signalling the failure mode to the caller.

The result: cross-link citations that reference large sections of source code or prose produce `None` verifications, which the repair pipeline interprets as degraded and may prune. Users never learn that the 4KB cap was the cause.

## What Changes

- Restructure `verify_cited_span` (the function that calls `best_fuzzy_match`) to try a cascade of matchers in increasing cost, bailing out at the first high-confidence hit:
  1. **Exact substring match on normalised text** — `O(N + M)` via SQLite-style `strstr` / Rust `String::find`. Already done as a post-check; promote it to pre-check. Ratio 1.0 on hit.
  2. **Anchored fuzzy match** — a rolling-hash or `strstr`-of-first-N-chars-then-extend pass that locates candidate offsets without full word-window enumeration.
  3. **Windowed LCS fallback** — the current algorithm, but with the 4KB cap lifted and a soft time budget (e.g., 50 ms) instead of a hard size cap.
- Replace the hard `if original_source.len() > 4096 { return None; }` guard with the soft time budget. Emit a `tracing::warn!` when the budget trips so the operator sees it.
- Keep the output shape (`(offset, ratio)`) and the 0.9-ratio acceptance threshold unchanged.
- Add integration tests that exercise:
  - 10KB source with exact needle substring (fast path)
  - 10KB source with paraphrased needle (fallback path)
  - 100KB source (stresses the time budget)

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `repair-loop`: `cross_link_verify` no longer silently drops large inputs. Verification for symbol bodies > 4KB now succeeds on exact-match cases and attempts fuzzy matching within a time budget for approximate cases.

## Impact

- **Code**:
  - `src/pipeline/repair/cross_link_verify.rs` — rewrite `verify_cited_span` cascade, relax `best_fuzzy_match` size cap, add time-budget guard.
  - New tests covering the three cascade paths.
- **APIs**: No change to the exported `verify_cited_span` signature or the `CitedSpan` shape.
- **Dependencies**: Potentially add a rolling-hash helper or use `memchr` (already a transitive dep via `regex`); verify during implementation.
- **Systems**: None.
- **Docs**: No changes to `AGENTS.md` / `CLAUDE.md`. The submodule `AGENTS.md` at `src/pipeline/repair/` lists `cross_link_verify.rs` but does not document the 4KB cap.
