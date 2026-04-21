# Synthesis providers

The synthesis pipeline supports multiple LLM providers for commentary and cross-link generation.

**Disabled by default.** Synthesis is off even when provider API keys are present in the environment, so `synrepo` never silently consumes keys set for unrelated tools. Enable it via `[synthesis]` in `.synrepo/config.toml`, by running `synrepo setup` (the interactive wizard configures it), or by passing `synrepo setup <tool> --synthesis` (scripted flow + synthesis sub-wizard).

| Provider | Env var | Default model | API key |
|----------|---------|---------------|---------|
| Anthropic (default) | `ANTHROPIC_API_KEY` | `claude-sonnet-4-6` | Required |
| OpenAI | `OPENAI_API_KEY` | `gpt-4o-mini` | Required |
| Gemini | `GEMINI_API_KEY` | `gemini-1.5-flash` | Required |
| OpenRouter | `OPENROUTER_API_KEY` | `google/gemma-4-31b-it:free` | Required |
| Z.ai (Zhipu GLM) | `ZAI_API_KEY` | `glm-4.6` | Required |
| MiniMax | `MINIMAX_API_KEY` | `MiniMax-M2` | Required |
| Local (Ollama/llama.cpp/LM Studio/vLLM) | `SYNREPO_LLM_LOCAL_ENDPOINT` | `llama3` | None |

## Config block

All fields optional, serde-defaulted so older configs load unchanged:

```toml
[synthesis]
enabled = true
provider = "anthropic"    # "anthropic" | "openai" | "gemini" | "openrouter" | "zai" | "minimax" | "local" | "none"
model = "claude-sonnet-4-6"
local_endpoint = "http://localhost:11434/api/chat"
local_preset = "ollama"   # informational only; local_endpoint is authoritative
```

## Precedence (env wins)

- `SYNREPO_LLM_ENABLED=1` overrides `enabled = false`
- `SYNREPO_LLM_PROVIDER` > `synthesis.provider` > default (`anthropic`)
- `SYNREPO_LLM_MODEL` > `synthesis.model` > provider default
- `SYNREPO_LLM_LOCAL_ENDPOINT` > `synthesis.local_endpoint` > `http://localhost:11434/api/chat`
- Unknown provider strings fall back to `anthropic` with a warning; the same applies to an unknown `synthesis.provider` value in config

For `Local`, the request shape is inferred from the endpoint path: `/v1/chat/completions` → OpenAI-compatible (llama.cpp, LM Studio, vLLM); any other path → Ollama native. No dedicated implementation per server is needed.

## API key handling

API keys live in the shell environment only. `synrepo` does not write keys to `.synrepo/config.toml` or any persisted state; OS-keychain integration is explicitly out of scope today.

The legacy `SYNREPO_ANTHROPIC_API_KEY` is also accepted as a fallback to `ANTHROPIC_API_KEY`.

User-facing rationale (what synthesis produces, when to enable, rough cost) lives in `README.md` under "Optional LLM synthesis" — keep operator-only details here and narrative there.

## Telemetry

Per-call telemetry lands in `.synrepo/state/synthesis-log.jsonl` and aggregates in `.synrepo/state/synthesis-totals.json`. Each log record carries timestamp, provider, model, duration, input/output tokens, `usage_source`, `usd_cost`, and outcome. The log rotates on `synrepo sync --reset-synthesis-totals`.

Token counts flagged `est.` came from a local OpenAI-compatible server that did not return a `usage` block; the accounting layer marks those calls `UsageSource::Estimated` end-to-end and the Health tab exposes `any_estimated: true` so estimated and reported numbers never get rolled into a single "accurate" total.

Pricing in `src/pipeline/synthesis/pricing.rs` has a `LAST_UPDATED` date; unknown `(provider, model)` pairs record `usd_cost: null` rather than guess.
