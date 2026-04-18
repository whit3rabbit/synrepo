# stage4-call-scope-narrowing-v1 baseline metrics

## Synrepo (current)

**Date:** 2026-04-18

### Calls edge count
- Total `Calls` edges (active): 10,637
- Unique callee targets: 1,304
- Average fan-out: 8.15 edges per unique callee

### Top-20 most-fan-out short names (by unique target count)
| Short name | Edge count | Example qualified names |
|------------|------------|-------------------------|
| new | 153 | `ClaudeCommentaryGenerator::new`, `HandoffsRequest::new`, `ModelResolver::new`, ... |
| get | (see below) | |
| map | (see below) | |

The top-20 are dominated by common constructors (`::new`) and factory methods.

### Observations
- Fan-out is high because any `new` call connects to every `new` in the repo
- This makes neighborhood expansion noisy for short names

## External corpora
- Not captured (flask/axios not available locally)
- Can be added later if needed

## Task completion
- [x] 1.1 Synrepo baseline captured
- [ ] 1.2 External corpus (skipped - not practical without local copies)
- [ ] 1.3 This notes file created