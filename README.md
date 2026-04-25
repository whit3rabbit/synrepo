# synrepo

[![CI](https://github.com/whit3rabbit/synrepo/actions/workflows/ci.yml/badge.svg)](https://github.com/whit3rabbit/synrepo/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/synrepo.svg)](https://crates.io/crates/synrepo)
[![Built With Ratatui](https://img.shields.io/badge/Built_With_Ratatui-000?logo=ratatui&logoColor=fff)](https://ratatui.rs/)

`synrepo` gives coding agents a compact, queryable map of your repository so they can stop reading source cold and start with the smallest useful context.

It builds a local structural model of the repo, keeps it fresh, and exposes it through read-only MCP tools plus a guided dashboard. It is not session memory and it is not a task tracker. It is repo intelligence for coding agents.

The default agent loop is explicit: orient, find bounded cards, inspect impact, edit, validate tests, then review changed context. Synrepo responses are budgeted by default. Large source files are not the default response unit unless a caller explicitly escalates.

## Why Use It

- Start agents from cards, search, entrypoints, and impact views instead of dumping full files into prompts.
- See context accounting on card responses: estimated card tokens, raw-file comparison tokens, source hashes, and truncation state.
- Keep repository context local, inspectable, and tied to the actual codebase under `.synrepo/`.
- Use one operational flow: `setup` wires the repo and agent, bare `synrepo` opens the guided UI, `watch` keeps the model fresh, and `status` tells you whether things are healthy.
- Keep machine-authored commentary separate from canonical repo facts so the trust boundary stays visible.

## Quick Start

### Install

- macOS (Homebrew): `brew install whit3rabbit/tap/synrepo`
- macOS and Linux (script): `curl -fsSL https://raw.githubusercontent.com/whit3rabbit/synrepo/main/scripts/install.sh | sh`
- Windows (PowerShell): `irm https://raw.githubusercontent.com/whit3rabbit/synrepo/main/scripts/install.ps1 | iex`
- Cargo: `cargo install synrepo`
- Direct downloads: [releases page](https://github.com/whit3rabbit/synrepo/releases)

The install scripts verify downloads against the release `SHA256SUMS` before installing. On macOS they use Homebrew when `brew` is on `PATH` unless `SYNREPO_SKIP_BREW=1` is set. Otherwise the shell script installs to `~/.local/bin`, and the PowerShell script installs to `%LOCALAPPDATA%\synrepo\`.

### First Run

In your repository root:

```bash
synrepo setup
synrepo setup <tool>
```

Then, on a ready repo:

```bash
synrepo
```

- `synrepo setup` initializes `.synrepo/`, writes the agent instructions or skill file, and registers project-scoped MCP where that integration is automated.
- `synrepo` probes the repo and routes to setup, repair, or the dashboard based on current state.
- `synrepo watch --daemon` is the normal follow-up if you want the repo model to stay fresh while you edit.

## How It Fits Together

- `synrepo setup <tool>`: install synrepo into the repo and wire your agent.
- `synrepo`: open the guided operator UI. On a ready repo, this lands in the dashboard.
- `synrepo watch --daemon`: keep the local repo model fresh as files change. Cheap repair surfaces (export regeneration, retired-observation compaction) auto-run after each drift-producing reconcile when `auto_sync_enabled = true` (the default in `.synrepo/config.toml`).
- `synrepo status`: verify health, freshness, and whether anything needs attention.
- `synrepo mcp`: serve read-only repo intelligence to the agent over stdio.
- The dashboard Explain tab refreshes advisory commentary when you want machine-authored summaries for missing or stale areas.

## Daily Workflow

1. Run `synrepo` or `synrepo dashboard` to inspect the current repo state.
2. Keep `synrepo watch --daemon` running while you work if you want automatic refresh.
3. Use `synrepo status` when you want a quick health check.
4. Let the agent query `synrepo_orient`, `synrepo_find`, `synrepo_explain`, `synrepo_impact`, `synrepo_tests`, and `synrepo_changed` instead of opening large files first.
5. Use `synrepo explain <target> --budget 1000`, `synrepo impact <target> --budget 2000`, or `synrepo tests <path> --budget 1500` for the same flow outside MCP.
6. Use the dashboard Explain tab if you want fresh commentary on the parts of the repo that just moved.
7. Use `synrepo check` and `synrepo sync` when health or repair surfaces need manual attention.

## Trust Model

- Parser-observed code facts live in the graph. That graph is the canonical repo model.
- Machine-authored commentary and link suggestions live in a separate overlay store. They are advisory, never canonical.
- If graph facts and commentary disagree, trust the graph.
- Commentary refresh is explicit and opt-in. `synrepo` does not silently spend provider budget just because API keys exist in your shell.
- Accepted cross-link suggestions become human-declared graph edges. Unaccepted suggestions remain advisory overlay data.

## Supported Agent Integrations

`synrepo setup <tool>` supports two integration tiers:

| Tier | Tools | What synrepo does | What you do |
|---|---|---|---|
| Automated | `claude`, `codex`, `cursor`, `windsurf`, `open-code`, `roo` | Initializes `.synrepo/`, writes the repo-local skill or instruction file, and registers the project-scoped MCP server in the agent's local config | Start the agent in the repo |
| Shim-only | `copilot`, `generic`, `gemini`, `goose`, `kiro`, `qwen`, `junie`, `tabnine`, `trae` | Initializes `.synrepo/` and writes the repo-local skill or instruction file | Point the agent at `synrepo mcp --repo .` in that tool's own MCP config |

For Codex, `synrepo setup codex` writes the skill to `.agents/skills/synrepo/SKILL.md` and registers MCP in trusted project `.codex/config.toml` using `[mcp_servers.synrepo]`. For a global Codex registration with the installed binary, run `codex mcp add synrepo -- synrepo mcp --repo .`; for an npm-distributed build, run `codex mcp add synrepo -- npx -y synrepo mcp --repo .`. You can also add the same table to `~/.codex/config.toml`.

Use `synrepo agent-setup <tool>` if you only want to regenerate the instruction file without running the full onboarding flow.

## Command Cheat Sheet

| Command | What it is for |
|---|---|
| `synrepo setup <tool>` | First-time install for a repo and agent |
| `synrepo` | Guided entrypoint that routes to setup, repair, or dashboard |
| `synrepo watch --daemon` | Keep the repo model fresh in the background |
| `synrepo status` | Quick operational health check |
| `synrepo mcp` | Serve read-only repo intelligence to the agent |
| Dashboard Explain tab | Refresh advisory commentary for missing or stale areas |
| `synrepo check` | Read-only drift and repair report |
| `synrepo sync` | Apply auto-fixable repair actions |
| `synrepo doctor` | Degraded-component-only report; non-zero exit for CI / pre-commit gates |
| `synrepo handoffs` | Prioritized actionable items from repair log, cross-link candidates, and git hotspots |
| `synrepo search "query"` | Lexical search through the repo index |
| `synrepo cards --query "task" --budget 1500` | Bounded card suggestions for a task |
| `synrepo explain <target> --budget 1000` | Bounded card for a file or symbol |
| `synrepo impact <target> --budget 2000` | Change risk before editing |
| `synrepo tests <path> --budget 1500` | Test-surface discovery |
| `synrepo stats context --json` | Context-serving metrics |
| `synrepo bench context --tasks "benches/tasks/*.json" --json` | Reproducible context-savings benchmark |
| `synrepo graph stats` | Inspect graph node and edge counts |

## Evidence For Context-Savings Claims

`synrepo bench context` produces the evidence that backs numeric context-savings statements in this README, release notes, and product docs. A numeric context-savings percentage cites a benchmark run and reports reduction ratio, target hit rate, miss rate, stale rate, and latency. Token reduction on its own is not a savings claim: a small card that misses required context is a regression, not a win.

Qualitative wording (for example "bounded structural cards", "smaller than raw-file reads") does not need a benchmark run. Numeric percentages do.

```bash
synrepo bench context --tasks "benches/tasks/*.json" --json
```

The checked-in fixture set under `benches/tasks/` covers route-to-edit, symbol explanation, impact or risk, and test-surface discovery. Missing categories are reported in the benchmark summary rather than silently ignored, so release reviewers can see which workflows are not exercised.

The report carries a `schema_version` field and stable field names. Patch releases keep the field shape compatible; a rename or removal bumps the schema version.

## Optional Advisory Commentary

Everything above works locally without any LLM. Advisory commentary and cross-link suggestion generation are separate, opt-in features.

- The dashboard Explain tab refreshes machine-authored commentary for files and symbols that are missing summaries or have gone stale.
- `synrepo sync --generate-cross-links` and `synrepo sync --regenerate-cross-links` generate machine-authored suggestions linking design docs to code.
- Commentary and link suggestions stay in the overlay store. They never become canonical graph facts unless a human explicitly accepts a suggested link.

### Providers

| Provider | Env var for key | Default model |
|---|---|---|
| Anthropic | `ANTHROPIC_API_KEY` | `claude-sonnet-4-6` |
| OpenAI | `OPENAI_API_KEY` | `gpt-4o-mini` |
| Gemini | `GEMINI_API_KEY` | `gemini-1.5-flash` |
| OpenRouter | `OPENROUTER_API_KEY` | `google/gemma-4-31b-it:free` |
| Z.ai (Zhipu GLM) | `ZAI_API_KEY` | `glm-4.6` |
| MiniMax | `MINIMAX_API_KEY` | `MiniMax-M2` |
| Local (Ollama, llama.cpp, LM Studio, vLLM) | none | `llama3` |

API keys stay in your shell environment. `synrepo` does not write them into `.synrepo/config.toml` or any on-disk cache.

### Enable It

Add an `[explain]` block to `.synrepo/config.toml`, or let `synrepo setup` configure it through the optional explain sub-wizard:

```toml
[explain]
enabled = true
provider = "anthropic"   # or "openai" | "gemini" | "openrouter" | "zai" | "minimax" | "local"
# model = "claude-sonnet-4-6"
# local_endpoint = "http://localhost:11434/api/chat"
```

Local quick starts:

- Ollama: `ollama serve && ollama pull llama3`, then set `local_endpoint = "http://localhost:11434/api/chat"`
- llama.cpp, LM Studio, or vLLM (OpenAI-compatible): set `local_endpoint = "http://localhost:8080/v1/chat/completions"`

One-shot env overrides:

- `SYNREPO_LLM_ENABLED=1`
- `SYNREPO_LLM_PROVIDER=anthropic|openai|gemini|openrouter|zai|minimax|local|none`
- `SYNREPO_LLM_MODEL=<model>`
- `SYNREPO_LLM_LOCAL_ENDPOINT=<url>`

### When To Turn It On

- You want fast file or symbol summaries while onboarding a new operator or agent.
- You keep human-authored design docs under `docs/adr/`, `docs/concepts/`, or `docs/decisions/` and want link suggestions back to the code.
- You already run a local model or you are comfortable spending a small amount on cloud commentary refresh.

### Usage And Accounting

- Every commentary call appends to `.synrepo/state/explain-log.jsonl`.
- Aggregated totals live in `.synrepo/state/explain-totals.json`.
- `synrepo sync --reset-explain-totals` rotates the log and zeroes the totals snapshot.
- The dashboard surfaces live commentary activity and accumulated usage in the Live and Health views.

## Supported Languages

Structural extraction is wired for five languages. Parser output becomes graph facts, and stage-4 resolution promotes imports and call references into cross-file edges where the language contract supports it.

| Language | Extensions | Symbols extracted | Signature + docs | Import resolution |
|---|---|---|---|---|
| Rust | `.rs` | `fn`, `struct`, `enum`, `trait`, `type`, `mod`, `const`, `static`, `impl` methods | Yes (`///` line comments) | `use` paths not yet resolved to files |
| Python | `.py` | `def`, `class`, methods, nested defs | Yes (docstring) | Dotted imports resolve to `a/b.py` |
| TypeScript | `.ts` | `function`, `class`, methods, `interface`, `type` alias | Yes (JSDoc `/** */`) | Relative paths resolve to target files |
| TSX | `.tsx` | TypeScript symbols plus JSX-bearing components | Yes (JSDoc `/** */`) | Relative paths resolve to target files |
| Go | `.go` | `func`, methods, `struct`, `interface` | No | Imports not yet resolved to files |

Markdown concept extraction runs separately on `concept_directories`, which default to `docs/concepts/`, `docs/adr/`, and `docs/decisions/`.

## How synrepo compares

synrepo is not trying to replace session memory tools or task trackers. Its job is narrower and more repo-centric.

- Compared with session memory tools such as `claude-mem`, synrepo focuses on the repository itself, not chat history.
- Compared with portable memory tools such as `memvid`, synrepo is heavier to explain but much stronger on code structure, provenance, and freshness.
- Compared with task systems such as `beads`, synrepo helps agents understand the codebase, not coordinate project execution.

If your main problem is "help the agent understand this repo without brute-forcing full files," synrepo is the stronger fit.

## Reference Docs

- [docs/FOUNDATION.md](docs/FOUNDATION.md): product intent, trust model, and design boundaries
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): layer architecture and storage layout
- [docs/CONFIG.md](docs/CONFIG.md): config fields and defaults
- [docs/EXPLAIN.md](docs/EXPLAIN.md): explain providers, API keys, and telemetry
- [docs/ADDING-LANGUAGE.md](docs/ADDING-LANGUAGE.md): adding a new tree-sitter language

<details>
<summary>Developer</summary>

### Build

```bash
cargo build
make build
```

### Validate

```bash
make check
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

### Run

```bash
cargo run -- --help
cargo run -- mcp
```

</details>
