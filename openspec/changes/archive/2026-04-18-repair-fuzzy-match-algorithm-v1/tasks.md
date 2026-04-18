## 1. Baseline benchmark

- [x] 1.1 Capture a baseline trace of `verify_cited_span` wall-time for the existing fuzzy-match path on representative inputs (small, medium, at-the-cap, post-cap/dropped). Use a throwaway `#[bench]`-style test or `criterion` if convenient.
- [x] 1.2 Record the baseline numbers in the task log — informs Stage C budget tuning.

## 2. Add Stage A (exact substring fast path)

- [x] 2.1 In `verify_cited_span`, normalise source and needle, then run `normalized_source.find(&normalized_needle)` before calling `best_fuzzy_match`.
- [x] 2.2 On hit, construct and return the `CitedSpan` with `lcs_ratio = 1.0` and `verified_at_offset` = the found byte offset.
- [x] 2.3 Confirm the 1.0-ratio post-check inside `best_fuzzy_match` is now redundant; leave or remove consistently.
- [x] 2.4 Run `cargo test --lib pipeline::repair::cross_link_verify::` and confirm existing tests still pass.

## 3. Add Stage B (anchored partial match)

- [x] 3.1 Choose an anchor from the normalised needle — first 16 bytes after trimming leading whitespace is acceptable for v1.
- [x] 3.2 Find every occurrence of the anchor in the normalised source (`memchr`-style linear scan or `find` in a loop).
- [x] 3.3 For each hit, evaluate LCS on a window of `needle.len() + 32` bytes anchored at the hit. Track the best ratio.
- [x] 3.4 Apply the 10 ms soft budget on the per-hit verification loop.
- [x] 3.5 Return `(offset, ratio)` for any hit with ratio ≥ 0.9. Otherwise fall through to Stage C.

## 4. Modify `best_fuzzy_match` for Stage C

- [x] 4.1 Delete the `if original_source.len() > 4096 || needle.len() > 256 { return None; }` guard at line 172.
- [x] 4.2 Add a `started: Instant` at function entry and check `started.elapsed() > Duration::from_millis(50)` every 32 outer-loop iterations.
- [x] 4.3 On budget trip, `tracing::warn!` with source length, needle length, and whether a best match was found; return the best-so-far tuple or `None`.
- [x] 4.4 Rename the function to `windowed_lcs_match` to reflect its narrowed role, or leave the name — pick during implementation.

## 5. Logging

- [x] 5.1 Emit `tracing::debug!` at the entry of `verify_cited_span` with the stage cascade decision (A hit, B hit, C fallback).
- [x] 5.2 Emit `tracing::warn!` on Stage C budget trip (from task 4.3) and on Stage B budget trip (task 3.4 if it trips).

## 6. Tests

- [x] 6.1 Add `verify_exact_substring_in_large_source` per design D4.
- [x] 6.2 Add `verify_paraphrase_in_large_source` per design D4.
- [x] 6.3 Add `verify_budget_trip_returns_best_so_far` per design D4. If wall-time budgets are flaky, gate with `#[cfg(not(miri))]` or replace with an operation-count budget.
- [x] 6.4 Re-run any existing tests that asserted `best_fuzzy_match` returned `None` for > 4 KB input — they now return `Some(_)` or `None` based on content, not size. Update assertions accordingly.

## 7. Verification

- [x] 7.1 Run `make check` and confirm fmt, clippy, and the full test suite pass.
- [x] 7.2 Smoke-test: run `synrepo sync --revalidate-links` on a corpus with large-body cross-links (synrepo itself has several; verify at least one that was previously silently dropped now validates).
- [x] 7.3 Compare repair-log JSONL output before and after for the same corpus; confirm the "degraded" count for cross-link surfaces drops.

## 8. Archive

- [x] 8.1 Run `openspec validate repair-fuzzy-match-algorithm-v1 --strict`.
- [ ] 8.2 Invoke `opsx:archive` with change id `repair-fuzzy-match-algorithm-v1`.
