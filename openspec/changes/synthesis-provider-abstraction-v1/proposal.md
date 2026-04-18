## Why

The synthesis pipeline has two live generators, both hardcoded to Anthropic:

- `src/pipeline/synthesis/claude.rs:23` — `const API_URL: &str = "https://api.anthropic.com/v1/messages";`
- `src/pipeline/synthesis/cross_link_claude.rs:25` — same URL, same version constant.

Each reads `SYNREPO_ANTHROPIC_API_KEY` (`claude.rs:26`, `cross_link_claude.rs:28`) and uses a blocking `reqwest::Client` with duplicated timeout, chars-per-token, model-name, and response-parsing logic. The `CommentaryGenerator` and `CrossLinkGenerator` traits (`src/pipeline/synthesis/mod.rs:44–47`) are already the correct abstraction — there is no architectural reason the project is single-provider, only implementation inertia.

Consequences of staying single-provider:

- Users with OpenAI, Gemini, or a local LLM (Ollama, vLLM) cannot exercise the synthesis pipeline at all. `NoOpGenerator` is not a substitute — it silently skips generation.
- Provider outages, deprecations, or billing changes are a hard dependency on Anthropic.
- The two `*_claude.rs` files duplicate HTTP plumbing that would live in a shared helper if multiple providers existed.

The fix is to rename the Claude-specific files back to the provider-agnostic path they already have traits for, and add at least one sibling implementation so the abstraction is load-bearing rather than aspirational.

## What Changes

- Introduce a `Provider` selection layer in `src/pipeline/synthesis/mod.rs`:
  - Read `SYNREPO_LLM_PROVIDER` env var; values: `anthropic` (default), `openai`, `gemini`, `local`, `none`.
  - Factory function `pub fn build_commentary_generator(max_tokens_per_call: u32) -> Box<dyn CommentaryGenerator>` dispatches on the provider.
  - Symmetric factory for cross-link: `pub fn build_cross_link_generator(max_tokens_per_call: u32, thresholds: ConfidenceThresholds) -> Box<dyn CrossLinkGenerator>`.
- Move provider-specific code into sibling modules:
  - `src/pipeline/synthesis/providers/anthropic.rs` — rename of today's `claude.rs` and `cross_link_claude.rs` combined by type of output.
  - `src/pipeline/synthesis/providers/openai.rs` — new. Calls `https://api.openai.com/v1/chat/completions` with `OPENAI_API_KEY`. Chat Completions API, same JSON-in/JSON-out shape.
  - `src/pipeline/synthesis/providers/gemini.rs` — new. Calls `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent` with `GEMINI_API_KEY`.
  - `src/pipeline/synthesis/providers/local.rs` — new. Calls `http://localhost:11434/api/chat` by default (Ollama); configurable via `SYNREPO_LLM_LOCAL_ENDPOINT`. No auth header.
  - `src/pipeline/synthesis/providers/http.rs` — shared helper: timeout constant, chars-per-token calc, blocking client builder, common response-parse skeleton. Keeps each provider file <200 lines.
- Deprecate `SYNREPO_ANTHROPIC_API_KEY` in favor of `ANTHROPIC_API_KEY` (standard Anthropic SDK env var). Keep the old name as an alias for one release with a `tracing::warn!` on access. Add the alias table to CLAUDE.md so operators can migrate.
- Introduce a `Provider::Config` struct that carries the common inputs (API key, base URL override, model name, timeout) so each provider constructor is short and uniform.
- Add a `ProviderKind` enum in `src/pipeline/synthesis/providers/mod.rs` so `synrepo status --full` can report which provider is active.
- Gate the change on user approval before merging (architectural change, introduces new env vars and deps).

## Capabilities

### New Capabilities

None at the user-visible level. The new providers unlock existing `synthesis` capability for non-Anthropic users.

### Modified Capabilities

- `synthesis`: no longer Anthropic-only. `CommentaryGenerator` and `CrossLinkGenerator` traits stay unchanged; the set of wired implementations grows.
- `configuration`: new env vars `SYNREPO_LLM_PROVIDER`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `SYNREPO_LLM_LOCAL_ENDPOINT`, `SYNREPO_LLM_MODEL`.
- `operational status`: `synrepo status` gains a `synthesis_provider` line.

## Impact

- **Code**:
  - New directory `src/pipeline/synthesis/providers/` with `mod.rs`, `anthropic.rs`, `openai.rs`, `gemini.rs`, `local.rs`, `http.rs`, `config.rs`.
  - `src/pipeline/synthesis/mod.rs` — rewrite to export the factory functions and `ProviderKind`; drop the `pub mod claude` / `pub mod cross_link_claude` lines.
  - Delete `src/pipeline/synthesis/claude.rs` and `src/pipeline/synthesis/cross_link_claude.rs`.
  - `src/pipeline/synthesis/cross_link/mod.rs` (existing) — unchanged; it uses the trait, not the Anthropic impl directly.
  - `src/pipeline/repair/commentary.rs` — update the constructor call from `ClaudeCommentaryGenerator::new_or_noop(...)` to `build_commentary_generator(...)`.
  - `src/bin/cli_support/commands/status.rs` — add `synthesis_provider: ProviderKind` to the status payload; render in the text and JSON modes.
- **APIs**: No public `pub` type rename; `ClaudeCommentaryGenerator` and `ClaudeCrossLinkGenerator` were used downstream only by `repair/commentary.rs` and the cross-link repair path, both internal. Re-export aliases for backward compat if any external consumer depends on the old names (check by grep — likely no, since this is an application, not a library).
- **Storage**: No change. `CommentaryProvenance.model_identity` already carries the model string, so provenance of generated rows remains specific to the provider that produced them.
- **Dependencies**: No new deps. `reqwest` already covers the surface; response-shape parsing uses `serde_json::Value` within each provider to keep the dep tree unchanged.
- **Docs**:
  - `AGENTS.md` — add the `SYNREPO_LLM_PROVIDER` table to the Config fields section or a new "Synthesis providers" subsection.
  - `docs/FOUNDATION-SPEC.md` — the "synthesis pipeline" section currently calls out Claude specifically; generalize to provider-agnostic.
  - Add a one-page migration note for the env var rename (`SYNREPO_ANTHROPIC_API_KEY` → `ANTHROPIC_API_KEY`).
- **Systems**: No change to daemons, watch service, or storage layout. The synthesis pipeline is a cold path; provider switch is per-process env-var.

## User approval gate

Per the roadmap, this change is marked as requiring user ask before implementation. Two specific decisions need operator sign-off:

1. **Env var alias window**. How long should `SYNREPO_ANTHROPIC_API_KEY` continue to work? Proposed: one release (next minor version emits a `tracing::warn!` telling users to migrate; the release after that removes the alias).
2. **Dependency list**. No new deps are proposed. If we decide to adopt a higher-level client (e.g., `async-openai`, `llm`), that is a separate change with its own approval.

Do not begin implementation until both are confirmed.
