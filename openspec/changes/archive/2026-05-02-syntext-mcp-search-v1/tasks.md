## 1. MCP Contract

- [x] 1.1 Extend `synrepo_search` parameters with optional path, type, exclude, and case-insensitive filters.
- [x] 1.2 Route MCP search through `search_with_options` and include syntext/source-store metadata in the response.
- [x] 1.3 Preserve the old minimal `{ query, limit }` call shape and response fields.

## 2. Documentation

- [x] 2.1 Document the expanded `synrepo_search` contract in MCP docs.
- [x] 2.2 Document the explicit freshness policy for MCP search.

## 3. Verification

- [x] 3.1 Add handler tests for MCP path, glob, type, exclude, case-insensitive, alias, limit, and backward-compatible searches.
- [x] 3.2 Run focused Rust tests for search and MCP.
- [x] 3.3 Validate the OpenSpec change.
