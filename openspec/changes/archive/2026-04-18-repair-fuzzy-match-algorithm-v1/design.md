## Context

Cross-link verification scans the source file (or symbol body) cited by an LLM-produced link to confirm the quoted span actually appears, within fuzzy tolerance. The current implementation:

1. Normalises both source and needle (lowercase, collapse whitespace).
2. For every word boundary in the normalised source, extends a window of `needle_word_count ± 2` words.
3. Computes LCS ratio (byte-DP, O(win * needle)) for each window.
4. Returns the best window.
5. If the final ratio ≥ 0.9, checks whether the normalised needle is an exact substring of the normalised source; upgrades ratio to 1.0 if so.

The size guard (`len > 4096 || len > 256`) at the top of `best_fuzzy_match` silently returns `None` for anything larger. This is a safety guard, not a design feature — it exists because the algorithm's cost grows as `N * (window_count) * O(win * M)` ≈ `O(N * M²)`, which is ~268M byte-ops at the cap.

**Key observation**: in the common case, the LLM cites a verbatim span of the source, so step 5 (exact-substring post-check) would already find it with `O(N + M)` cost — but the algorithm insists on running step 3 first. Swapping the order removes most of the cost for the common case.

## Goals / Non-Goals

**Goals:**

- Eliminate the 4KB silent-drop guard for exact and near-exact matches.
- Keep the fallback fuzzy path bounded so it cannot hang the repair loop on pathological inputs.
- Preserve the 0.9-ratio acceptance threshold and the `(offset, ratio)` output contract.

**Non-Goals:**

- No change to the citation acceptance policy. A ratio below 0.9 still yields `None`.
- No change to the `CitedSpan` schema or the repair-surface caller.
- No introduction of heavyweight fuzzy-match crates (e.g., `triple_accel`, `aho-corasick`). Stay in-tree with cheap primitives.
- No concurrency / parallelism within the match. Called in a sequential repair loop; optimise per-call cost only.

## Decisions

### D1: Cascade: exact-substring first, then anchored, then windowed LCS

Rewrite `verify_cited_span` to try matchers in order:

**Stage A — Exact substring (normalised).** Use `String::find` on the normalised source for the normalised needle. Cost `O(N + M)` with a tight constant factor. If found, return `(offset, 1.0)` immediately. This handles the common verbatim-citation case even for sources much larger than 4KB.

**Stage B — Anchored partial match.** Pick the longest continuous-run substring of the needle (say, the 16-byte window with the fewest unique characters or simply the first 16 chars after trimming) and `String::find` it in the normalised source. For each hit offset, run LCS on a localised `needle.len() ± 32` window around the hit. Returns the highest ratio among hits. Cost `O(N + k·M²)` where k is the number of anchor hits, typically 1–5.

**Stage C — Windowed LCS fallback (current algorithm, relaxed).** Only reached when stages A and B produce no ratio ≥ 0.9. Same word-boundary windowed LCS, but with:

- The size cap removed.
- A soft time budget (50 ms default) enforced by checking `Instant::now()` inside the outer loop every ~32 iterations.
- A `tracing::warn!` logged when the time budget trips, reporting source length and needle length.

**Rationale**. The cascade exploits the common-case cheap path without sacrificing the fallback. Stage A alone handles verbatim quotes, which empirically dominate LLM output. Stage B handles common paraphrase patterns (punctuation changes, stopword drops). Stage C is the rarely-taken tail.

**Alternatives considered:**

- *Replace the whole algorithm with Myers' diff*: rejected — Myers is good for computing edit distance between two strings of similar length, not for locating a small needle inside a large source. The "locate needle" step still needs a scanning pass.
- *Use a rolling hash for the entire needle*: rejected — adds complexity and does not handle punctuation drift well. The normalised source already collapses whitespace and case, which is the most common drift.
- *Use `aho-corasick` from the `regex` ecosystem*: rejected — adds a direct dep for a feature that needs multi-pattern matching, which we do not have.

### D2: Soft time budget over hard size cap

A time budget generalises across input shapes. Short + weird inputs can be as expensive as long + well-structured inputs; a size cap only protects the long case. The budget fires rarely in practice because stages A/B handle the vast majority of inputs first.

Budget values:

- Stage A: unbounded (it is already `O(N + M)`).
- Stage B: 10 ms soft cap on the anchor-verification loop.
- Stage C: 50 ms soft cap on the full windowed LCS pass.

On budget trip, Stage C returns the best-so-far ratio (or `None` if none evaluated), not `None` unconditionally.

### D3: Logging on budget trip or degraded path

Emit a `tracing::warn!` with:

- The offset of the first anchor hit in Stage B (if any).
- Source and needle lengths.
- The stage that produced the returned ratio (A/B/C).

This makes the silent-drop-mode from the old cap visible in repair logs, which the current code denies the operator.

### D4: Tests

Add in `src/pipeline/repair/cross_link_verify.rs` (or a new `cross_link_verify/tests.rs` module):

- `verify_exact_substring_in_large_source`: 10 KB source with a 200-byte verbatim needle. Expect ratio = 1.0, offset correct.
- `verify_paraphrase_in_large_source`: 10 KB source with a 200-byte needle that has one-word changes. Expect ratio between 0.9 and 1.0 via Stage B.
- `verify_budget_trip_returns_best_so_far`: 500 KB source with a 250-byte needle that will trip the Stage C budget. Expect either a valid match (if one was found before the trip) or `None`.
- Retain or restore coverage for the 4 KB legacy case: confirm behaviour matches the new cascade and does not regress.

## Risks / Trade-offs

- **Stage B's anchor choice is heuristic**: picking the first 16 bytes of the normalised needle may land on a common substring (e.g., "the user agent"). Mitigation: use the longest run of consonants or highest-entropy 16-byte window instead; simple if needed. Measure Stage B false-hit rate in tests before complicating.

- **Time budgets introduce non-determinism**: a slow test machine could trip the budget on a test that a fast machine completes. Mitigation: bump budgets during tests (`#[cfg(test)]` override) or use a fixed operation count instead of wall time. Decide in implementation.

- **`tracing::warn!` volume on degenerate repair runs**: if many cross-links hit Stage C, the warn volume could be high. Mitigation: rate-limit per `verify_cited_span` call or demote to `debug` once the pattern is observed. Start with `warn!` for visibility; tune later.

- **Stage A's `String::find` is byte-based, not Unicode-aware**: the existing `normalize_text` already forces ASCII-like normalisation (whitespace collapse, lowercase); non-ASCII text will behave as it does today. Mitigation: none needed for v1.

## Migration Plan

Single PR. No data migration. Behaviour change is strictly additive (larger inputs now succeed; smaller inputs still succeed). Repair-log JSONL records (`src/pipeline/repair/log.rs`) do not encode the stage, so no downstream schema change.

## Open Questions

- **O1**: Should the Stage C time budget be `Duration::from_millis(50)` (current proposal), `100`, or operator-configurable? Start with 50 ms; revisit after benchmarking on real cross-link corpora.
- **O2**: Should the fix also update the 0.9 acceptance threshold? Out of scope for this change — that is a policy decision, not an algorithm fix.
