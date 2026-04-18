## Context

The trait boundaries in `src/pipeline/synthesis/mod.rs` (`CommentaryGenerator`, `CrossLinkGenerator`) were designed to be provider-agnostic. The actual code under `synthesis/` has two files (`claude.rs`, `cross_link_claude.rs`) that both hit `https://api.anthropic.com/v1/messages` and read `SYNREPO_ANTHROPIC_API_KEY`. The HTTP plumbing, response parsing, and retry/timeout logic are duplicated. The caller in `src/pipeline/repair/commentary.rs` calls `ClaudeCommentaryGenerator::new_or_noop(...)` directly — there is no factory, so swapping providers is a source change.

The fix is mechanical: move provider-specific code into provider-specific modules, introduce a factory that reads an env var, and write at least one non-Anthropic provider so the trait shape is exercised.

## Goals / Non-Goals

**Goals:**

- Any `CommentaryGenerator` / `CrossLinkGenerator` consumer uses a factory that respects `SYNREPO_LLM_PROVIDER`; no consumer hardcodes `ClaudeCommentaryGenerator`.
- At least one non-Anthropic provider (OpenAI) ships working. Gemini and local are stretch goals for this change; all four are listed below, but shipping OpenAI alone is enough to validate the abstraction.
- Each provider module stays under 200 lines. Shared HTTP plumbing lives in `providers/http.rs`.

**Non-Goals:**

- Streaming responses. Commentary and cross-link generation are short; a single blocking request is still the right shape.
- Tool use / function calling. Not used by either generator today.
- Multi-provider fallback (e.g., try OpenAI, fall back to local on error). One provider per process. Reasoning: simpler failure mode, less ambiguous debugging.
- Token-cost accounting per provider. `max_tokens_per_call` remains a chars-based heuristic; each provider applies it uniformly.
- Replacing `reqwest::blocking` with async. Synthesis runs on the same thread as `synrepo sync`, which is synchronous. Keeping blocking minimizes runtime surface.

## Decisions

### D1: Directory layout

```
src/pipeline/synthesis/
├── mod.rs                      # trait + factory; 120 lines max
├── providers/
│   ├── mod.rs                  # ProviderKind enum, Config struct, build_* factories
│   ├── http.rs                 # shared: blocking client builder, timeout, chars-per-token, `HttpRequest` helper
│   ├── anthropic.rs            # Claude Messages API — commentary + cross-link paths
│   ├── openai.rs               # OpenAI Chat Completions — commentary + cross-link paths
│   ├── gemini.rs               # Google Gemini generateContent
│   └── local.rs                # Ollama / vLLM (OpenAI-compatible local endpoints)
├── cross_link/                 # unchanged: scoring, evidence verification, prompt templates
└── stub.rs                     # unchanged: NoOpGenerator, NoOpCrossLinkGenerator
```

Rationale: each provider file holds both `impl CommentaryGenerator` and `impl CrossLinkGenerator` for that provider (currently `claude.rs` and `cross_link_claude.rs` are split by trait; keeping them separate means 8 provider files instead of 4). Duplicated API URLs and response parsing inside one provider is the smallest change.

### D2: Factory functions, no global state

```rust
// providers/mod.rs
pub enum ProviderKind { Anthropic, OpenAi, Gemini, Local, None }

impl ProviderKind {
    fn from_env() -> Self {
        match std::env::var("SYNREPO_LLM_PROVIDER").ok().as_deref() {
            Some("anthropic") | None => ProviderKind::Anthropic,
            Some("openai") => ProviderKind::OpenAi,
            Some("gemini") => ProviderKind::Gemini,
            Some("local") => ProviderKind::Local,
            Some("none") => ProviderKind::None,
            Some(other) => {
                tracing::warn!("unknown SYNREPO_LLM_PROVIDER '{other}', falling back to anthropic");
                ProviderKind::Anthropic
            }
        }
    }
}

pub fn build_commentary_generator(max_tokens_per_call: u32) -> Box<dyn CommentaryGenerator> { ... }
pub fn build_cross_link_generator(...) -> Box<dyn CrossLinkGenerator> { ... }
```

The factory dispatches on `ProviderKind::from_env()`, looks up the per-provider API key (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, or `None` for local), and returns the matching impl or the `NoOp` fallback when the key is missing.

**Why env-var driven rather than config-file driven**: synthesis is opt-in and cold-path. Users who want multi-provider setups (dev uses local, CI uses OpenAI) already have env-var conventions. Adding a `[synthesis]` block to `.synrepo/config.toml` is a separate, larger change if ever warranted.

### D3: Env var mapping

| Concern | Env var | Default |
|---------|---------|---------|
| Provider selection | `SYNREPO_LLM_PROVIDER` | `anthropic` |
| Anthropic key | `ANTHROPIC_API_KEY` (alias: `SYNREPO_ANTHROPIC_API_KEY` — deprecated one release) | (unset → NoOp) |
| OpenAI key | `OPENAI_API_KEY` | (unset → NoOp) |
| Gemini key | `GEMINI_API_KEY` | (unset → NoOp) |
| Local endpoint | `SYNREPO_LLM_LOCAL_ENDPOINT` | `http://localhost:11434` |
| Model name override | `SYNREPO_LLM_MODEL` | per-provider default |

**Why `ANTHROPIC_API_KEY` not `SYNREPO_ANTHROPIC_API_KEY`**: matches the env var that Anthropic's official SDK reads, matches what `claude` and `claude-code` use. Users already have `ANTHROPIC_API_KEY` exported in their shell; making synrepo agree reduces friction.

**Alias window**: `SYNREPO_ANTHROPIC_API_KEY` is read as a fallback; if set, emit a one-time `tracing::warn!` telling users to rename. Keep the alias for one release, then remove. Confirm the timeline with the user before implementation.

### D4: Shared HTTP helper

`providers/http.rs` exposes:

```rust
pub(super) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const CHARS_PER_TOKEN: u32 = 4;

pub(super) fn build_client() -> reqwest::blocking::Client { ... }

pub(super) fn estimate_tokens(context: &str) -> u32 { ... }

pub(super) fn post_json<Req: Serialize, Res: DeserializeOwned>(
    client: &reqwest::blocking::Client,
    url: &str,
    headers: &[(&str, &str)],
    body: &Req,
) -> crate::Result<Option<Res>> { ... }
```

Each provider module uses `post_json` with its own request/response types. Non-success status, JSON parse failure, and timeout all map to `Ok(None)` with a `tracing::warn!` — same contract the current `claude.rs` has.

### D5: Preserve `CommentaryEntry` and `OverlayLink` output shapes

Both provider functions return `Option<CommentaryEntry>` / `Option<OverlayLink>` exactly as they do today. No caller changes. The only semantic change is `model_identity` carrying strings like `gpt-4o-mini`, `gemini-1.5-flash`, `llama3` instead of only `claude-sonnet-4-6`. Existing overlay filters keyed on `model_identity` keep working; cross-tool federation becomes possible.

### D6: Status surface

`src/bin/cli_support/commands/status.rs` adds one line:

```
synthesis provider: openai (model: gpt-4o-mini)
```

or:

```
synthesis provider: none  (set SYNREPO_LLM_PROVIDER to enable)
```

In JSON mode: `"synthesis_provider": "openai"`, `"synthesis_model": "gpt-4o-mini"`. Computed via a new `describe_active_provider()` helper in `providers/mod.rs`.

### D7: Tests

- `providers/anthropic.rs` tests: port the existing two tests from `claude.rs` and `cross_link_claude.rs`. They assert constructor safety and oversized-context skip. No network; mock not needed.
- `providers/openai.rs` tests: same shape. Constructor tolerates missing key (returns NoOp), oversized context skips, the actual HTTP call remains untested (matches the existing Claude test — we do not run network tests in CI).
- `providers/mod.rs` tests: factory dispatch. `from_env` with each of the five values (including `none` and an invalid value) returns the expected `ProviderKind`. Factory returns `NoOp` when no key is set.
- Integration test in `pipeline/repair/commentary.rs`: confirm the refactored caller still works with `NoOpGenerator` (the `#[ignore]` network-dependent tests stay `#[ignore]`).

Avoid any test that would require network access. The provider factories must be constructible offline.

## Risks / Trade-offs

- **OpenAI Chat Completions is a different shape than Anthropic Messages**. System prompt goes in a `messages[0]` with `role: "system"`, not a top-level `system` field. Response has `choices[0].message.content`, not `content[0].text`. The provider module handles the translation; the trait contract stays identical. Risk: subtle output drift between providers for the same prompt. Mitigation: each provider logs its model_identity, so users comparing outputs see why they differ.

- **Local / Ollama JSON shape varies by deployment**. Ollama's `/api/chat` returns a different envelope than vLLM's OpenAI-compatible endpoint. Decision: `local` provider targets OpenAI-compatible endpoints (both Ollama 0.5+ and vLLM ship this). Users with non-compatible local servers add a provider module themselves.

- **`reqwest::blocking` runs on the caller's thread**. The synthesis pipeline is called from the repair loop, which can now block for 30 seconds per request. No change from today, but worth reiterating. Users running `synrepo sync` with a slow provider (or a broken local endpoint) will see latency. Mitigation: the existing `max_tokens_per_call` already skips oversized contexts.

- **Env var sprawl**. Six new env vars. Mitigation: `SYNREPO_LLM_PROVIDER` is the one that drives everything; the per-provider keys are named after the standard conventions so most users already have them. Document the matrix in one table in AGENTS.md.

- **Alias window for `SYNREPO_ANTHROPIC_API_KEY`**: users who script around synrepo may break on removal. Mitigation: emit `tracing::warn!` per process (not per call) so the deprecation is visible; keep the alias for one release.

## Migration Plan

Single PR, sequencing:

1. Create `providers/` directory with `mod.rs`, `http.rs`, `anthropic.rs`. Implementation of `anthropic.rs` is a copy of `claude.rs` + `cross_link_claude.rs` merged and ported to the shared `http::post_json` helper.
2. Switch `src/pipeline/repair/commentary.rs` and `cross_link/*` callers to the factory. Ship this step alone, tagged as "no behavior change": same provider, same env var.
3. Add `openai.rs`. Now users can opt in by setting `SYNREPO_LLM_PROVIDER=openai` + `OPENAI_API_KEY`.
4. Add `gemini.rs` and `local.rs` in the same PR or follow-up.
5. Delete `claude.rs` and `cross_link_claude.rs`.

Env var migration:

- On process start, if `SYNREPO_ANTHROPIC_API_KEY` is set and `ANTHROPIC_API_KEY` is not, copy the value and warn.
- One release later, remove the alias.

Rollback: revert the PR. No data migration. Overlay commentary / cross-link rows are keyed on `model_identity`, which remains correct across rollback.

## Open Questions

- **O1**: Do we want `SYNREPO_LLM_MODEL` as a single var, or per-provider model vars (`SYNREPO_LLM_ANTHROPIC_MODEL`, etc.)? Current proposal: single var. Users who switch providers re-set it; simpler mental model.
- **O2**: Should the factory emit a `tracing::info!` at process start naming the active provider + model? Current proposal: yes, once per process, at info level. Cheap operational visibility.
- **O3**: Local endpoint default: `http://localhost:11434` (Ollama) or `http://localhost:8000/v1` (vLLM OpenAI-compat default)? Current proposal: Ollama's `/api/chat`. Users with vLLM set `SYNREPO_LLM_LOCAL_ENDPOINT` explicitly.
- **O4**: Should `ProviderKind::None` exist, or should users set `SYNREPO_LLM_PROVIDER=anthropic` + leave the key unset to disable? Current proposal: keep `None` as explicit opt-out for scripts that want to ensure synthesis never runs.
