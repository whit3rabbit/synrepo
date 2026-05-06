# synrepo

[![CI](https://github.com/whit3rabbit/synrepo/actions/workflows/ci.yml/badge.svg)](https://github.com/whit3rabbit/synrepo/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/synrepo.svg)](https://crates.io/crates/synrepo)
[![Built With Ratatui](https://img.shields.io/badge/Built_With_Ratatui-000?logo=ratatui&logoColor=fff)](https://ratatui.rs/)

`synrepo` gives coding agents a compact, queryable map of your repository so they can stop reading source cold and start with the smallest useful context.

It builds a local structural model of the repo, keeps it fresh, and exposes it through read-only MCP tools plus a guided dashboard. Its product model is `repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP`: graph facts are canonical, artifacts are compiled records, task contexts are bounded bundles, and cards/MCP are the delivery packets agents consume. It is not session memory and it is not a task tracker. It is repo intelligence for coding agents. See [docs/MCP.md](docs/MCP.md) for the MCP workflow and tool surface.

The default agent loop is explicit: orient, find bounded cards, inspect impact, edit, validate tests, then review changed context. Synrepo responses are budgeted by default. Large source files are not the default response unit unless a caller explicitly escalates.

## Why Use It

- Start agents from cards, search, entrypoints, and impact views instead of dumping full files into prompts.
- Give agents task contexts assembled from graph-backed code artifacts, without making them crawl raw files first.
- See context accounting on card responses: estimated card tokens, raw-file comparison tokens, source hashes, and truncation state.
- Keep repository context local, inspectable, and tied to the actual codebase under `.synrepo/`.
- Use one operational flow: `setup` wires the repo and agent, bare `synrepo` opens the guided UI, `watch` keeps the model fresh, and `status` tells you whether things are healthy.
- Keep machine-authored commentary separate from canonical repo facts so the trust boundary stays visible.

## Quick Start

### 1. Install

Install `synrepo` once per machine:

- macOS (Homebrew): `brew install --cask whit3rabbit/tap/synrepo`
- macOS and Linux (script): `curl -fsSL https://raw.githubusercontent.com/whit3rabbit/synrepo/main/scripts/install.sh | sh`
- Windows (PowerShell): `irm https://raw.githubusercontent.com/whit3rabbit/synrepo/main/scripts/install.ps1 | iex`
- Cargo: `cargo install synrepo`
- Direct downloads: [releases page](https://github.com/whit3rabbit/synrepo/releases)

The install scripts verify downloads against the release `SHA256SUMS` before installing. On macOS they use Homebrew when `brew` is on `PATH` unless `SYNREPO_SKIP_BREW=1` is set. Otherwise the shell script installs to `~/.local/bin`, and the PowerShell script installs to `%LOCALAPPDATA%\synrepo\`.

### 2. Set Up A Repo

```bash
cd /path/to/repo
synrepo setup claude
```

`synrepo setup <tool>` is the scripted onboarding path. It initializes `.synrepo/`, writes the agent skill or instructions, records the repo in `~/.synrepo/projects.toml`, runs the first reconcile, and registers MCP when the target supports automated registration.

Run `synrepo setup` without a tool to launch the interactive TTY wizard. Run bare `synrepo` any time after that to probe the repo and route to setup, repair, or the dashboard.

Set up more than one client in one pass when needed:

```bash
synrepo setup --only claude,codex
synrepo setup --skip goose,kiro
```

Setup defaults to user-global MCP config when the target supports it. Global MCP entries launch `synrepo mcp` and serve managed projects by `repo_root`.

Use project-scoped MCP config when you want this repo to carry its own MCP entry:

```bash
synrepo setup claude --project
```

Project-scoped MCP launches `synrepo mcp --repo .`, so agent calls can omit `repo_root`.

Use `synrepo agent-setup <tool>` only when you want to regenerate the skill or instructions file. It does not initialize the runtime, run reconcile, register MCP, or do full onboarding.

### 3. Keep It Fresh

```bash
synrepo watch --daemon
```

MCP does not start watch for you. Run `synrepo watch --daemon` explicitly when you want the local model to stay fresh as files change. Cheap repair surfaces, such as export regeneration and retired-observation compaction, auto-run after drift-producing reconciles when `auto_sync_enabled = true` in `.synrepo/config.toml`.

## How It Fits Together

There are two separate scopes to keep straight:

- A managed project is a repository recorded in the user registry at `~/.synrepo/projects.toml`. Global MCP only serves managed projects, and MCP tools must pass the current workspace as `repo_root` unless the server was started with `synrepo mcp --repo <path>`.
- A discovery root is a filesystem root inside one managed project. The primary checkout is always indexed. Linked worktrees are indexed by default, initialized submodules are opt-in, and each discovery root has its own file identity domain.

Register another repository for an existing global MCP setup:

```bash
cd /path/to/another-repo
synrepo project add .
```

`synrepo project add [path]` bootstraps `.synrepo/` if needed, verifies the repo is ready, and records it in the registry. Use `synrepo project list`, `inspect`, `remove`, `use`, `rename`, and `prune-missing` to manage that registry. `synrepo project prune-missing` is a dry run unless `--apply` is passed.

Within one repo, use `docs/CONFIG.md` to tune discovery. The relevant defaults are `include_worktrees = true`, `include_submodules = false`, and `roots = ["."]`.

## Optional Embeddings

Embeddings are off by default. When synrepo is built with `semantic-triage`, you can enable them per repo from the dashboard Actions tab with `T`, then run `R` or `synrepo reconcile` to build the vector index.

Use `synrepo bench search --tasks 'benches/tasks/*.json' --mode both --json` to compare lexical and hybrid search before keeping embeddings on. On this repo, local Ollama `all-minilm` improved the four-fixture hit@5 baseline from `0.25` to `1.00`, with total search latency rising from `49 ms` to `690 ms`. See [docs/EMBEDDINGS.md](docs/EMBEDDINGS.md) for ONNX, Ollama, Hugging Face-hosted model artifacts, and benchmark guidance.

## MCP And Agent Setup

`synrepo mcp` is a stdio MCP server. It serves repository context, not background maintenance.

```bash
synrepo mcp                    # server for current repo when started there
synrepo mcp --repo <path>      # server with a default repo
synrepo mcp --allow-overlay-writes # expose overlay note/commentary writes
synrepo mcp --allow-source-edits   # expose anchored source edit tools
```

MCP does not start `synrepo watch`, scan every managed repo, run reconcile, or silently refresh commentary. Searches and cards read the current `.synrepo/` state. Use `synrepo watch`, `synrepo reconcile`, `synrepo check`, or `synrepo sync` when state needs maintenance.

The default agent workflow is:

1. `synrepo_orient`
2. `synrepo_ask` for one bounded, cited task-context packet
3. `synrepo_find` or exact `synrepo_search` for drill-down
4. `synrepo_explain`
5. `synrepo_impact` or `synrepo_risks`
6. `synrepo_tests`
7. `synrepo_changed`

Use `synrepo_minimum_context` when a focal target is known but the surrounding neighborhood is still unclear. Use `synrepo_context_pack` when several known read-only code artifacts or task-context pieces are cheaper in one response. The full MCP tool surface, resources, overlay tools, and edit-gated behavior live in [docs/MCP.md](docs/MCP.md).

## Daily Workflow

1. Run `synrepo` or `synrepo dashboard` to inspect the current repo state.
2. Keep `synrepo watch --daemon` running while you work if you want automatic refresh.
3. Use `synrepo status` when you want a quick health check.
4. Let the agent query `synrepo_orient`, `synrepo_ask`, `synrepo_find`, `synrepo_explain`, `synrepo_impact` or `synrepo_risks`, `synrepo_tests`, and `synrepo_changed` instead of opening large files first.
5. Use `synrepo_context_pack` when the agent needs several known read-only code artifacts or task-context pieces in one token-accounted response.
6. Use `synrepo explain <target> --budget 1000`, `synrepo impact <target> --budget 2000`, or `synrepo tests <path> --budget 1500` for the same flow outside MCP.
7. Use the dashboard Explain tab if you want fresh commentary on the parts of the repo that just moved.
8. Use `synrepo check` and `synrepo sync` when health or repair surfaces need manual attention.

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
| Automated MCP | `claude`, `codex`, `cursor`, `copilot`, `gemini`, `junie`, `open-code`, `qwen`, `roo`, `tabnine`, `windsurf` | Initializes `.synrepo/`, writes the skill or instructions, records the project, runs first reconcile, and registers MCP through `agent-config` | Start the agent in the repo |
| Shim or instructions only | `generic`, `goose`, `kiro`, `trae` | Initializes `.synrepo/`, writes the skill or instructions, records the project, and runs first reconcile | Point the agent at `synrepo mcp --repo .` in that tool's own MCP config if it supports MCP |

`synrepo setup <tool>` prefers the agent's global config when `agent-config` supports it. Global MCP entries launch `synrepo mcp`; repo-local entries launch `synrepo mcp --repo .`. Pass `--project` when you want repo-local MCP config. Legacy unowned setup artifacts can be adopted into the ownership ledger with `synrepo upgrade --apply`.

Global MCP serves managed projects only. `synrepo setup <tool>` records the current repo automatically. Later, use `synrepo project add <path>` for additional repos and `synrepo project prune-missing --apply` to clean registry entries for paths that no longer exist.

MCP usage details, including resources, advisory overlay tools, and edit-gated tools, live in [docs/MCP.md](docs/MCP.md).

Use `synrepo agent-setup <tool>` if you only want to regenerate the skill or instructions file without running the full onboarding flow.

## Remove Or Uninstall

Use `synrepo remove` to plan removal of synrepo-owned artifacts from the current repo without uninstalling the binary. Pass `--apply` to execute the plan. Bulk `synrepo remove` targets tracked or detected agent skills, instructions, MCP entries, Git hooks, root `.gitignore` lines synrepo added, registry rows, and optionally `.synrepo/` itself.

Use `synrepo uninstall` for the guided full teardown across projects, integrations, global state, data, and binary removal. Project `.synrepo/` directories and `~/.synrepo` database/cache files are kept by default; pass `--delete-data` after reviewing the dry run to include them. Direct binaries installed by the script can be deleted at the end when the path is safe, while Homebrew and Cargo installs print the exact uninstall command.

## Command Cheat Sheet

| Command | What it is for |
|---|---|
| `synrepo` | Guided entrypoint that routes to setup, repair, or dashboard |
| `synrepo setup [tool]` | Interactive wizard without a tool, scripted repo and agent setup with a tool |
| `synrepo setup --only claude,codex` | Set up several supported clients in one pass |
| `synrepo agent-setup <tool>` | Regenerate only the skill or instructions file |
| `synrepo project add [path]` | Register and bootstrap another managed project for global MCP |
| `synrepo project list` | List managed projects and current health |
| `synrepo project inspect [path]` | Check whether a repo is managed and ready |
| `synrepo project prune-missing [--apply]` | Dry-run or apply cleanup for missing managed repos |
| `synrepo watch --daemon` | Keep the repo model fresh in the background |
| `synrepo status` | Quick operational health check |
| `synrepo mcp` | Serve read-only repo intelligence to the agent |
| `synrepo mcp --repo <path>` | Serve MCP with a default repo so `repo_root` can be omitted |
| `synrepo mcp --allow-overlay-writes` | Expose overlay note/commentary write tools |
| `synrepo mcp --allow-source-edits` | Expose anchored source edit tools |
| `synrepo remove [--apply]` | Dry-run or apply removal of synrepo-owned artifacts from the current repo |
| `synrepo uninstall` | Guided full teardown |
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
| `synrepo bench context --tasks "benches/tasks/*.json" --mode all --json` | Reproducible context-quality benchmark |
| `synrepo graph stats` | Inspect graph node and edge counts |

## Evidence For Context-Savings Claims

`synrepo bench context --mode all` produces the evidence that backs numeric context-savings or context-quality statements in this README, release notes, and product docs. A numeric context claim cites a benchmark run and reports reduction ratio, target hit rate, miss rate, stale rate, latency, task success, token return, citation coverage, span coverage, and wrong-context rate when an allow-list is present. Token reduction on its own is not a savings claim: a small card that misses required context is a regression, not a win.

Qualitative wording (for example "bounded structural cards", "smaller than raw-file reads") does not need a benchmark run. Numeric percentages do.

```bash
synrepo bench context --tasks "benches/tasks/*.json" --mode all --json
```

The checked-in fixture set under `benches/tasks/` covers route-to-edit, symbol explanation, impact or risk, and test-surface discovery. Missing categories are reported in the benchmark summary rather than silently ignored, so release reviewers can see which workflows are not exercised. The default `--mode cards` preserves the historical cards benchmark aliases; use `--mode ask` to compare `synrepo_ask` against cards, and `--mode all` to include raw-file and lexical baselines.

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

API keys can come from provider environment variables or user-global config. The setup wizard saves entered cloud keys in `~/.synrepo/config.toml` so they can be reused across repos. Repo-local `.synrepo/config.toml` stores explain provider and model settings, not cloud API keys, and explain telemetry does not record keys.

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
- [docs/RUNTIME_BUDGET.md](docs/RUNTIME_BUDGET.md): runtime and storage budget review guardrails
- [docs/MCP.md](docs/MCP.md): MCP workflow, tool groups, resources, and edit-gated behavior
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
