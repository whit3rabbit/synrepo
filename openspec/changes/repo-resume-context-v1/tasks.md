## 1. Planning Artifacts

- [x] 1.1 Create proposal, design, and spec deltas for repo resume context.
- [x] 1.2 Verify OpenSpec change is apply-ready before code implementation.

## 2. Core Surface

- [x] 2.1 Extract changed-file discovery into a shared surface helper.
- [x] 2.2 Add a `surface::resume_context` collector with packet types, defaults, note summaries, detail pointers, and budget trimming.
- [x] 2.3 Record aggregate context metrics for resume packet responses without storing content.

## 3. Public Interfaces

- [x] 3.1 Add CLI command `synrepo resume-context` with markdown and JSON output.
- [x] 3.2 Add MCP tool `synrepo_resume_context`.
- [x] 3.3 Update docs, MCP README, doctrine, skill/shim text, and source-registration tests.

## 4. Validation

- [x] 4.1 Add unit tests for empty packets, notes, budget trimming, and unavailable overlay behavior.
- [x] 4.2 Add CLI and MCP registration/schema tests.
- [x] 4.3 Run focused tests, `cargo fmt --check`, and `make ci-lint`.
