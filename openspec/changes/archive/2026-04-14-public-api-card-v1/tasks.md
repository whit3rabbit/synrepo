# Tasks: public-api-card-v1

- [x] 1. Fix ROADMAP.md: `synrepo_module` → `synrepo_module_card` (4 occurrences)
- [x] 2. Create `openspec/changes/public-api-card-v1/` artifacts (this file + proposal + design)
- [x] 3. Add `PublicAPIEntry` and `PublicAPICard` to `src/surface/card/types.rs`
- [x] 4. Re-export both types and add `public_api_card` to `CardCompiler` trait in `src/surface/card/mod.rs`
- [x] 5. Change `classify_kind` to `pub(super)` in `src/surface/card/compiler/entry_point/mod.rs`
- [x] 6. Create `src/surface/card/compiler/public_api.rs` with `public_api_card_impl`
- [x] 7. Add `mod public_api;` and trait dispatch to `src/surface/card/compiler/mod.rs`
- [x] 8. Add `PublicAPICardParams` and `synrepo_public_api` tool to `crates/synrepo-mcp/src/main.rs`
- [x] 9. Run `INSTA_UPDATE=new cargo test public_api` to generate snapshots
- [x] 10. Run `make check` — all tests must pass
- [x] 11. Update `openspec/specs/cards/spec.md` — add PublicAPICard requirement section
- [x] 12. Update `openspec/specs/mcp-surface/spec.md` — register `synrepo_public_api`
- [x] 13. Update `ROADMAP.md §11.1` — move PublicAPICard to compiled, add synrepo_public_api to MCP list
- [x] 14. Update `skill/SKILL.md` — add `synrepo_public_api` tool row
