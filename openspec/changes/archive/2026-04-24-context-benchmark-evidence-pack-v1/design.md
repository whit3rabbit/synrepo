## Context

The context-accounting work shipped accounting metadata, metrics, workflow aliases, and a `bench context` command. The implementation currently has a harness but no repository-owned task corpus, so it can measure mechanics without proving usefulness across repeatable tasks.

## Goals / Non-Goals

**Goals:**

- Define benchmark fixtures that can be reviewed, versioned, and run in CI or release checks.
- Report both compression and usefulness: reduction ratio, expected-target hits, misses, stale rate, and latency.
- Keep output stable enough for README claims and regression diffs.

**Non-Goals:**

- No new retrieval algorithm.
- No LLM grading.
- No benchmark mutation of `.synrepo/` beyond ordinary read preparation and metrics side effects.
- No generic performance benchmark for all synrepo commands.

## Decisions

1. **Use checked-in JSON task fixtures.** Tasks live under `benches/tasks/` and define query text plus expected files, symbols, or tests. This keeps benchmark intent reviewable and avoids hidden local corpora.

2. **Measure baseline and card paths in one report.** Each task records raw-read baseline tokens, card tokens, reduction ratio, target hit/miss, stale rate, latency, and returned targets. A single schema prevents README numbers from mixing incompatible runs.

3. **Treat misses as first-class output.** A high token reduction with missing required context is a failure, not a success. Text output may summarize, but JSON must retain per-task miss detail.

4. **Keep claims gated by benchmark metadata.** Documentation may use qualitative wording without benchmark output. Numeric percentages require a named benchmark run and the accompanying hit-rate and stale-rate dimensions.

## Risks / Trade-offs

- Fixture tasks can become stale as the repo changes, mitigation: include expected-target validation and update fixtures in the same change that moves major surfaces.
- Small fixture sets can overfit, mitigation: require more than one task category and make missing categories visible in review.
- Token estimates are approximate, mitigation: report them as estimates and pair them with hit-rate evidence.
