## Context

The substrate already owns discovery policy and the persisted syntext index under `.synrepo/index/`. CLI search passes full `syntext::SearchOptions`, while MCP `synrepo_search` currently calls the default search wrapper and truncates results after the index query. Watch and reconcile already maintain the index explicitly, including incremental touched-path updates when the watch service has a trustworthy bounded path set.

## Goals / Non-Goals

**Goals:**
- Give MCP callers a grep/ripgrep-style exact search surface without adding another tool name.
- Preserve existing clients that call `synrepo_search` with only `query` and `limit`.
- Surface search provenance and active filters in the JSON response.
- Keep search read-only and rely on existing init/reconcile/watch freshness mechanisms.

**Non-Goals:**
- Do not add semantic search, regex search, or another index.
- Do not mutate the repo, trigger reconcile, or auto-start watch from MCP search.
- Do not redefine cards as syntext-backed truth; syntext remains routing and exact fallback.

## Decisions

- Extend `synrepo_search` instead of adding `synrepo_grep`. This keeps the MCP surface compact and preserves the existing workflow guidance.
- Convert MCP params to `syntext::SearchOptions` in `src/surface/mcp/search.rs`. This reuses the substrate filter behavior already covered by CLI tests and avoids duplicating matching policy in MCP.
- Pass `limit` into `SearchOptions.max_results` instead of truncating after search. This lets syntext bound work early and makes `limit` part of the query contract.
- Return metadata fields alongside existing response fields: `engine`, `source_store`, `limit`, `filters`, and `result_count`. Existing `query` and `results` remain unchanged.

## Risks / Trade-offs

- Filter naming could drift from CLI flags. Mitigation: use the same conceptual names as `SearchOptions` and accept `ignore_case` as an alias for `case_insensitive`.
- Search freshness can surprise users after file edits. Mitigation: document that MCP search is read-only and that `synrepo watch`, `synrepo reconcile`, init, and sync paths are the explicit refresh mechanisms.
- Result metadata may be mistaken for graph provenance. Mitigation: label `source_store` as `substrate_index`, distinct from graph and overlay stores.
