## 1. Specs And Docs

- [ ] 1.1 Add spec deltas for MCP search/routing outputs.
- [ ] 1.2 Add substrate hybrid-search and embedding chunk requirements.
- [ ] 1.3 Add card/test-risk and commentary estimate requirements.
- [ ] 1.4 Add graph identity and cross-link ranker requirements.
- [ ] 1.5 Update `docs/MCP.md`, `docs/CONFIG.md`, and `docs/ARCHITECTURE.md`.

## 2. Semantic Routing

- [ ] 2.1 Add `routing_strategy` and `semantic_score` to `TaskRoute`.
- [ ] 2.2 Add feature-gated semantic classifier with intent examples and centroid caching.
- [ ] 2.3 Wire MCP/CLI task routing through config-aware routing.
- [ ] 2.4 Add routing tests for safety precedence and fallback.

## 3. Hybrid Search And Embeddings

- [ ] 3.1 Add a substrate hybrid-search module with RRF fusion.
- [ ] 3.2 Add MCP/context-pack search `mode` with `auto` default and lexical fallback.
- [ ] 3.3 Improve symbol chunk text and bump vector index format.
- [ ] 3.4 Ensure query-time semantic loading does not download model assets.

## 4. Identity And Ranking

- [ ] 4.1 Add single-file rename detection with same-root guard.
- [ ] 4.2 Add bounded sampled-content similarity for symbol-poor files.
- [ ] 4.3 Add cross-link `RankFeatures` scorer module preserving current boundaries.

## 5. Backlog Surfaces

- [ ] 5.1 Add optional `risk_score` and `risk_reasons` to `TestEntry`.
- [ ] 5.2 Add estimated commentary freshness fields to `CommentaryCoverage`.

## 6. Verification

- [ ] 6.1 Run focused tests for touched modules.
- [ ] 6.2 Run `cargo fmt --check`.
- [ ] 6.3 Run `make ci-lint`.
- [ ] 6.4 Run `make ci-test`.
- [ ] 6.5 Run `cargo test --features semantic-triage` with local-only assets or mocks.
