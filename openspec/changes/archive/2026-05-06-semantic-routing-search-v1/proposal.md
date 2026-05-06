## Why

Agent-facing routing and search still depend mostly on exact phrases. That makes the fast path brittle: equivalent task language can miss deterministic edit routes, and lexical search can miss files whose symbols describe the requested concept without sharing the query words.

## What Changes

- Add semantic task routing behind the existing `semantic-triage` feature and `enable_semantic_triage` config. Deterministic safety guards remain authoritative, and routing falls back to keyword matching when semantic assets are unavailable.
- Add hybrid search for MCP and context-pack search artifacts: lexical syntext results plus embedding-index results fused with reciprocal rank fusion. Explicit lexical mode preserves prior exact-search behavior.
- Improve symbol embedding chunks so the vector index reflects qualified name, kind, path, signature, and doc comment, and bump the vector index format.
- Add structural single-file rename detection before split/merge, with bounded sampled content similarity for symbol-poor files and no unbounded byte LCS.
- Add a cross-link ranker module that preserves current score boundaries while making the feature extraction explicit.
- Surface cheap risk estimates on test entries and cheap commentary freshness estimates in default status.

## Non-Goals

- No SQLite DDL migration.
- No MinHash drift implementation.
- No cloud routing model or network download during task routing/search.
- No CRDT merge, A* call-path routing, CI-trained test prediction, or NL-to-graph-query.
