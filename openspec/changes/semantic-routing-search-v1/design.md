## Routing

`classify_task_route_with_config` runs deterministic safety and exact mechanical-edit checks first. When those do not decide the route, and semantic triage is compiled, enabled, and locally loadable, it embeds the task and compares it with cached intent centroids built from local example phrases. If any step fails, it returns the keyword result with `routing_strategy = "keyword_fallback"`.

Routing availability must never download model artifacts. Query-time semantic loading uses only existing cached model files.

## Search

Hybrid search lives in a substrate sibling module instead of growing `index.rs`. It gathers lexical top 100 and vector top 50, then applies reciprocal rank fusion with `k = 60`. Lexical rows keep `path`, `line`, and `content`; semantic-only rows expose source metadata but can leave line/content null.

`mode = "auto"` uses hybrid only when semantic triage is compiled, enabled, and locally loadable. `mode = "lexical"` keeps the previous syntext-only behavior.

## Embedding Chunks

Symbol chunk text includes stable symbol identity and human-readable context:

- qualified name
- kind
- file path when resolvable
- signature when available
- doc comment when available

The embedding index format version is bumped so stale vectors are rejected and rebuilt by reconcile.

## Identity

Stage 6 adds single-file rename detection before split/merge. It accepts a rename only when one unconsumed new file in the same discovery root clearly dominates the alternatives.

For symbol-rich files, detection uses exact Jaccard over symbol names. For symbol-poor files, it uses stored sampled-content shingle hashes with a hard file-size cap. It never runs full byte LCS in the hot path.

## Backlog Surfaces

Test risk scores are cheap graph/path heuristics, not learned CI predictions. Commentary freshness estimates are cheap aggregate signals and never replace exact `status --full` freshness.
