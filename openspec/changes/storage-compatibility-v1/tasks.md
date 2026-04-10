## 1. Define storage classes and compatibility checks

- [ ] 1.1 Classify current `.synrepo/` stores as canonical, supplemental, cached, or disposable according to the storage contract
- [ ] 1.2 Define compatibility metadata or checks needed to detect incompatible store formats and compatibility-sensitive config changes
- [ ] 1.3 Add tests for compatibility decisions such as rebuild-required, migrate-required, invalidate-cache, and safe-continue

## 2. Define maintenance and retention behavior

- [ ] 2.1 Align retention and cleanup expectations for index, graph, overlay, embeddings, cache, state, and logs with the storage contract
- [ ] 2.2 Define how later maintenance flows such as compact, cleanup, or upgrade must respect store classes and compatibility rules
- [ ] 2.3 Add tests or fixture-based validation for maintenance decisions where feasible

## 3. Align config and runtime bootstrap behavior

- [ ] 3.1 Review current config fields and mark which ones are compatibility-sensitive
- [ ] 3.2 Ensure bootstrap and runtime code can surface clear guidance when storage or config compatibility rules are violated
- [ ] 3.3 Validate the change with `openspec validate storage-compatibility-v1 --strict --type change`
