## 0. User approval gate

- [x] 0.1 Confirm with operator: deprecation timeline for `SYNREPO_ANTHROPIC_API_KEY` (proposed: one release with warn, then remove). **Decision: remove immediately since never released**
- [x] 0.2 Confirm with operator: no new direct deps (keep raw `reqwest`, serde_json, hand-rolled request/response types per provider).
- [x] 0.3 Confirm with operator: scope for the initial ship (recommended: Anthropic rename + OpenAI; Gemini and local in a follow-up). **Decision: all four providers in this change**

## 1. Scaffold providers/ directory

- [x] 1.1 Create `src/pipeline/synthesis/providers/mod.rs` with `ProviderKind`, `Config` struct, `from_env`, and stub factory functions that delegate to the existing code.
- [x] 1.2 Create `src/pipeline/synthesis/providers/http.rs` with `DEFAULT_TIMEOUT`, `CHARS_PER_TOKEN`, `build_client()`, `estimate_tokens()`, `post_json()`.
- [x] 1.3 Update `src/pipeline/synthesis/mod.rs` to `pub mod providers;` and re-export the factory functions.

## 2. Move Anthropic implementation

- [x] 2.1 Create `src/pipeline/synthesis/providers/anthropic.rs`.
- [x] 2.2 Port `ClaudeCommentaryGenerator::generate` body from `claude.rs` into a function that uses `http::post_json`. Keep the system prompt verbatim.
- [x] 2.3 Port `ClaudeCrossLinkGenerator::generate` body from `cross_link_claude.rs`. Keep scoring, thresholds, and CitedSpan handling verbatim.
- [x] 2.4 Read the API key from `ANTHROPIC_API_KEY`; if absent, try `SYNREPO_ANTHROPIC_API_KEY` as a legacy alias and emit a one-time `tracing::warn!("SYNREPO_ANTHROPIC_API_KEY is deprecated; set ANTHROPIC_API_KEY instead")`.
- [x] 2.5 Add unit tests mirroring the current `claude.rs` tests (constructor-no-panic, oversized-context-skip).

## 3. Wire the factory

- [x] 3.1 Implement `build_commentary_generator(max_tokens_per_call: u32) -> Box<dyn CommentaryGenerator>` in `providers/mod.rs`. Dispatch on `ProviderKind::from_env()`; if the corresponding key is unset, return `Box::new(NoOpGenerator)`.
- [x] 3.2 Implement `build_cross_link_generator(max_tokens_per_call: u32, thresholds: ConfidenceThresholds) -> Box<dyn CrossLinkGenerator>`.
- [x] 3.3 Add `describe_active_provider() -> (ProviderKind, &'static str)` returning the provider name and default model.

## 4. Update callers

- [x] 4.1 `src/pipeline/repair/commentary.rs` — replace `ClaudeCommentaryGenerator::new_or_noop(...)` with `build_commentary_generator(...)`.
- [x] 4.2 `src/pipeline/synthesis/cross_link/*` — replace any direct `ClaudeCrossLinkGenerator` construction with `build_cross_link_generator(...)`.
- [x] 4.3 Grep the codebase for `ClaudeCommentaryGenerator` and `ClaudeCrossLinkGenerator` — update every construction site.
- [x] 4.4 Keep `ClaudeCommentaryGenerator` and `ClaudeCrossLinkGenerator` as `pub use` re-exports in `providers/mod.rs` (or `synthesis/mod.rs`) for one release to avoid breaking any external code depending on the old names. Emit no deprecation warning yet — these are internal types for a binary, external breakage is unlikely.

## 5. Delete old files

- [x] 5.1 After step 4 compiles clean, delete `src/pipeline/synthesis/claude.rs` and `src/pipeline/synthesis/cross_link_claude.rs`.
- [x] 5.2 Remove their `pub mod` lines from `src/pipeline/synthesis/mod.rs`.

## 6. Add OpenAI provider

- [x] 6.1 Create `src/pipeline/synthesis/providers/openai.rs`.
- [x] 6.2 Implement `OpenAiCommentaryGenerator` using the Chat Completions API (`POST /v1/chat/completions`). System prompt goes in `messages[0]` with `role: "system"`. Response shape: `choices[0].message.content`. Default model: `gpt-4o-mini` (cheap + reliable).
- [x] 6.3 Implement `OpenAiCrossLinkGenerator` mirroring the Anthropic path.
- [x] 6.4 Read API key from `OPENAI_API_KEY`. Model override via `SYNREPO_LLM_MODEL`.
- [x] 6.5 Extend `build_commentary_generator` / `build_cross_link_generator` to dispatch to the OpenAI impl when `ProviderKind::OpenAi` is active.
- [x] 6.6 Unit tests: constructor-no-panic, oversized-context-skip. No network tests.

## 7. Add Gemini provider

- [x] 7.1 Create `src/pipeline/synthesis/providers/gemini.rs`. Endpoint: `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={key}`.
- [x] 7.2 Default model: `gemini-1.5-flash`.
- [x] 7.3 Wire into the factory.
- [x] 7.4 Tests mirror the Anthropic/OpenAI shape.

## 8. Add local provider

- [x] 8.1 Create `src/pipeline/synthesis/providers/local.rs`. Default endpoint: `http://localhost:11434/api/chat` (Ollama), overridable via `SYNREPO_LLM_LOCAL_ENDPOINT`.
- [x] 8.2 Assume OpenAI-compatible request/response when the endpoint path ends with `/v1/chat/completions`; otherwise assume Ollama native shape.
- [x] 8.3 No auth header.
- [x] 8.4 Tests: endpoint parsing, constructor.

## 9. Status surface

- [x] 9.1 In `src/bin/cli_support/commands/status.rs`, add a new field to the status struct: `synthesis_provider: String`, `synthesis_model: Option<String>`.
- [x] 9.2 Populate via `providers::describe_active_provider()`.
- [x] 9.3 Render in the text output and JSON output.

## 10. Docs

- [x] 10.1 Update `AGENTS.md` "Config fields" — add a "Synthesis providers" subsection with the env var table (Provider, Env var, Default).
- [x] 10.2 Update `docs/FOUNDATION-SPEC.md` synthesis pipeline section to reflect provider-agnostic wiring.
- [x] 10.3 Add a migration note paragraph: "Rename `SYNREPO_ANTHROPIC_API_KEY` to `ANTHROPIC_API_KEY`; the old name is accepted with a deprecation warning for one release."

## 11. Verification

- [x] 11.1 `make check` passes (fmt, clippy, parallel tests).
- [ ] 11.2 `cargo test --test mutation_soak -- --ignored --test-threads=1` passes (synthesis is off the writer path but confirm no regressions).
- [ ] 11.3 Smoke test per provider (with real API keys, not in CI):
  - Anthropic: `ANTHROPIC_API_KEY=... synrepo sync --refresh-commentary` on a small repo; confirm overlay commentary lands.
  - OpenAI: `SYNREPO_LLM_PROVIDER=openai OPENAI_API_KEY=... synrepo sync --refresh-commentary`; same confirmation.
  - Local (Ollama running): `SYNREPO_LLM_PROVIDER=local synrepo sync --refresh-commentary`.
- [ ] 11.4 Env var alias test: set only `SYNREPO_ANTHROPIC_API_KEY`; confirm synrepo reads it, emits the deprecation warn, and produces commentary.

## 12. Archive

- [ ] 12.1 Run `openspec validate synthesis-provider-abstraction-v1 --strict`.
- [ ] 12.2 Invoke `opsx:archive` with change id `synthesis-provider-abstraction-v1`.