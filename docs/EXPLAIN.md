# Explain providers

The explain pipeline supports multiple LLM providers for commentary and cross-link generation.

**Disabled by default.** Explain is off even when provider API keys are present in the environment, so `synrepo` never silently consumes keys set for unrelated tools. Enable it via `[explain]` in `.synrepo/config.toml`, by running `synrepo setup` (the interactive wizard configures it), or by passing `synrepo setup <tool> --explain` (scripted flow + explain sub-wizard).

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
[explain]
enabled = true
provider = "anthropic"    # "anthropic" | "openai" | "gemini" | "openrouter" | "zai" | "minimax" | "local" | "none"
model = "claude-sonnet-4-6"
local_endpoint = "http://localhost:11434/api/chat"
local_preset = "ollama"   # informational only; local_endpoint is authoritative
commentary_cost_limit = 5000
commentary_concurrency = 4
```

`commentary_cost_limit` is the approximate per-call input-token ceiling for
commentary generation. The default stays conservative to avoid surprise cost
or provider failures. For long-context providers, you can raise it toward
`150000` so explain can include more source, dependency, module, and test
context in one structured commentary document.

`commentary_concurrency` limits concurrent commentary provider calls during
refresh. It defaults to `4` and is clamped to at least `1`; set it to `1` for
strictly serial refreshes or providers with tight rate limits.

## Precedence (env wins)

- `SYNREPO_LLM_ENABLED=1` overrides `enabled = false`
- `SYNREPO_LLM_PROVIDER` > `explain.provider` > default (`anthropic`)
- `SYNREPO_LLM_MODEL` > `explain.model` > provider default
- `SYNREPO_LLM_LOCAL_ENDPOINT` > `explain.local_endpoint` > `http://localhost:11434/api/chat`
- Unknown provider strings fall back to `anthropic` with a warning; the same applies to an unknown `explain.provider` value in config

For `Local`, the request shape is inferred from the endpoint path: `/v1/chat/completions` → OpenAI-compatible (llama.cpp, LM Studio, vLLM); any other path → Ollama native. No dedicated implementation per server is needed.

## API key handling

API keys are resolved from provider environment variables first, then from user-global config. The setup wizard can save entered cloud keys in `~/.synrepo/config.toml` so they can be reused across repos. Saved keys are plaintext TOML on disk; file permissions depend on the host and umask. Prefer environment variables on shared machines, managed CI, or any host where other users may read your home directory.

Repo-local `.synrepo/config.toml` stores explain provider and model settings, not cloud API keys, and explain telemetry/accounting must not persist keys. OS-keychain integration is not implemented yet; see `docs/KEYCHAIN-DESIGN.md` for the intended migration design.

Local explain endpoints are treated as user-controlled provider endpoints. If `SYNREPO_LLM_LOCAL_ENDPOINT` or `explain.local_endpoint` points at a remote or untrusted service, that service can receive prompts containing source snippets, graph context, and commentary instructions. Only point local-provider config at endpoints you trust with the repository context.

The legacy `SYNREPO_ANTHROPIC_API_KEY` is also accepted as a fallback to `ANTHROPIC_API_KEY`.

User-facing rationale (what explain produces, when to enable, rough cost) lives in `README.md` under "Optional LLM explain" — keep operator-only details here and narrative there.

## Reviewing and editing commentary docs

Explain commentary is stored in the overlay database and can be materialized into editable Markdown:

```bash
synrepo docs export          # write .synrepo/explain-docs/ and update the explain-doc index
synrepo docs export --force  # rebuild explain-docs and explain-index from overlay
synrepo docs clean           # dry-run removal of materialized docs and index
synrepo docs clean --apply   # remove materialized docs and index; overlay is untouched
synrepo docs list            # list materialized docs and freshness
synrepo docs search <query>  # search materialized commentary docs
synrepo docs import <path>   # import one edited Markdown body back into the overlay
synrepo docs import --all    # import all edited Markdown bodies
```

`docs export` also writes native discovery files under
`.synrepo/explain-docs/`: `index.md` for a human/agent entrypoint,
`catalogue.json` for machine-readable metadata, and `llms.txt` for compact
LLM-facing discovery. These files are advisory, label `source_store: overlay`,
and point back to the materialized commentary docs and their graph-backed source
paths.

Only the body after the `---` separator is treated as editable content. Import skips a doc when its `source_content_hash` header no longer matches the current graph, so a stale edit cannot silently become fresh commentary for changed source.

Export always materializes from overlay commentary. That means local Markdown edits should be imported before running `docs export`, `docs export --force`, or an Explain refresh if the edits should be preserved. `docs list`, `docs import --all`, and `docs search` only treat `files/*.md` and `symbols/*.md` as editable/searchable commentary docs; discovery files are ignored by those commentary-specific operations. `docs clean --apply` deletes `.synrepo/explain-docs/` and `.synrepo/explain-index/`; it does not remove overlay commentary.

Newly generated commentary bodies use a fixed Markdown template: Purpose, How
It Fits, Associated Nodes, Important Gotchas, Associated Tests, TODOs / Dead
Code / Unfinished Work, Security Notes, and Context Confidence. Older free-form
overlay commentary remains valid until refreshed.

The dashboard Explain tab exposes the same maintenance actions: `d` exports docs without model calls, `D` force-rebuilds docs and the docs index, `x` previews a clean, and `X` removes the exported docs/index while leaving overlay commentary intact.

## Telemetry

Per-call telemetry lands in `.synrepo/state/explain-log.jsonl` and aggregates in `.synrepo/state/explain-totals.json`. Each log record carries timestamp, provider, model, duration, input/output tokens, `usage_source`, `usd_cost`, and outcome. Repo-scoped call paths set the accounting directory for the current operation so multi-repo MCP processes do not write explain totals to the last prepared repository. The log rotates on `synrepo sync --reset-explain-totals`.

Token counts flagged `est.` came from a local OpenAI-compatible server that did not return a `usage` block; the accounting layer marks those calls `UsageSource::Estimated` end-to-end and the Health tab exposes `any_estimated: true` so estimated and reported numbers never get rolled into a single "accurate" total.

Pricing in `src/pipeline/explain/pricing.rs` has a `LAST_UPDATED` date; unknown `(provider, model)` pairs record `usd_cost: null` rather than guess.
