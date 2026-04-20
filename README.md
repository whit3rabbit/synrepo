# synrepo

[![CI](https://github.com/whit3rabbit/synrepo/actions/workflows/ci.yml/badge.svg)](https://github.com/whit3rabbit/synrepo/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/synrepo.svg)](https://crates.io/crates/synrepo)
[![Built With Ratatui](https://img.shields.io/badge/Built_With_Ratatui-000?logo=ratatui&logoColor=fff)](https://ratatui.rs/)

`synrepo` is a Rust workspace with a CLI and an MCP server. The project is built around a few hard boundaries: parser-observed facts live in the graph, machine-authored output belongs in a separate overlay, and the user-facing product is small task-shaped context instead of dumping large files into prompts.

**synrepo is not a session-memory tool or a task tracker. It is a context compiler for coding agents that need a durable, inspectable understanding of the repository itself.**

## Installation

**macOS (Homebrew)**

    brew install whit3rabbit/tap/synrepo

**macOS and Linux (script)**

    curl -fsSL https://raw.githubusercontent.com/whit3rabbit/synrepo/main/scripts/install.sh | sh

Install a specific version:

    curl -fsSL https://raw.githubusercontent.com/whit3rabbit/synrepo/main/scripts/install.sh | INSTALL_VERSION=0.0.1 sh

**Windows (PowerShell)**

    irm https://raw.githubusercontent.com/whit3rabbit/synrepo/main/scripts/install.ps1 | iex

Install a specific version:

    & ([scriptblock]::Create((irm https://raw.githubusercontent.com/whit3rabbit/synrepo/main/scripts/install.ps1))) -Version 0.0.1

The scripts verify each download against the release `SHA256SUMS` before installing.
On macOS they use Homebrew when `brew` is on `PATH` (set `SYNREPO_SKIP_BREW=1` to skip).
Otherwise the Linux/macOS script installs to `~/.local/bin` and, if needed, adds that
directory to your shell rc (`~/.zshrc`, `~/.bashrc`, or `~/.profile`). The PowerShell
script installs to `%LOCALAPPDATA%\synrepo\` and updates your user `PATH`.

**Any platform with Cargo**

    cargo install synrepo

**Direct download**

Binaries for Linux (amd64, arm64), macOS (arm64, x86_64), and Windows (amd64) are available on the [releases page](https://github.com/whit3rabbit/synrepo/releases).

## What Exists Today

- CLI commands for `init`, `setup`, `status`, `reconcile`, `check`, `sync`, `search`, `graph`, `node`, `change-risk`, `watch`, `links`, `findings`, `handoffs`, `export`, `upgrade`, `compact`, `agent-setup`, and `mcp`
- A persisted `.synrepo/` workspace with lexical index, graph store, overlay store, config, and operational state
- Structural extraction for files, symbols, markdown concepts, and cross-file edges (Rust, Python, TypeScript/TSX, Go)
- Git-intelligence surfacing (history, hotspots, ownership, co-change) and change-risk assessment
- A watch service (`synrepo watch [--daemon]`) that keeps `.synrepo/` fresh as files change
- An MCP server (`synrepo mcp`) exposing 16 read-only tools for agent-facing repository context

## Optional LLM synthesis

Everything above runs locally without any LLM. Synthesis (commentary generation and cross-link candidate proposal) is a separate, opt-in path. It is **off by default**, even when provider API keys are present in your shell environment, so `synrepo` never silently consumes keys you set for other tools.

Supported providers:

| Provider | Env var for key | Default model |
|---|---|---|
| Anthropic | `ANTHROPIC_API_KEY` | `claude-sonnet-4-6` |
| OpenAI | `OPENAI_API_KEY` | `gpt-4o-mini` |
| Gemini | `GEMINI_API_KEY` | `gemini-1.5-flash` |
| Local (Ollama, llama.cpp, LM Studio, vLLM) | none | `llama3` |

API keys live in the shell environment only. `synrepo` never writes keys to `.synrepo/config.toml` or any on-disk cache.

**Enable synthesis** by adding a block to `.synrepo/config.toml` (or run `synrepo setup` and let the interactive wizard configure the `[synthesis]` block for you):

```toml
[synthesis]
enabled = true
provider = "anthropic"   # or "openai" | "gemini" | "local"
# model = "claude-sonnet-4-6"
# local_endpoint = "http://localhost:11434/api/chat"
```

**Local quick-starts**:

- Ollama: `ollama serve && ollama pull llama3`, then `local_endpoint = "http://localhost:11434/api/chat"`
- llama.cpp / LM Studio / vLLM (OpenAI-compatible): `local_endpoint = "http://localhost:8080/v1/chat/completions"`

The suffix `/v1/chat/completions` selects the OpenAI-compatible request shape; any other path uses Ollama's native `/api/chat` format.

**One-shot env overrides** (precedence: env > config > default):

- `SYNREPO_LLM_ENABLED=1` — enable without editing config
- `SYNREPO_LLM_PROVIDER=anthropic|openai|gemini|local|none`
- `SYNREPO_LLM_MODEL=<model>`
- `SYNREPO_LLM_LOCAL_ENDPOINT=<url>`

When synthesis is disabled but `synrepo` detects a provider key in your environment, `synrepo status` surfaces a one-line hint so you know the feature exists without guessing.

### What synthesis produces

Two artifacts, both stored in the overlay (never in the canonical graph) and always labeled machine-authored:

1. **Commentary** — a short paragraph describing what a file or symbol does. Real example from a refresh run against this repo's `writer` module:

   > writer.rs acquires a per-repo advisory lock on `.synrepo/state/writer.lock` via fs2 and retries briefly on WouldBlock to mask flock release latency. Holders stamp a JSON sidecar with pid + acquired_at for external diagnostics.

2. **Cross-link candidates** — proposed links between human-authored design docs and the code that implements them, tiered by confidence. You review and accept with `synrepo links accept`; accepted links become graph edges with `Epistemic::HumanDeclared`.

Nothing runs in the background. Trigger synthesis explicitly with `synrepo sync --generate-cross-links`, `synrepo sync --regenerate-cross-links`, or the `synrepo_refresh_commentary` MCP tool.

### When to enable it

- You have human-authored design docs under `docs/adr/`, `docs/concepts/`, or `docs/decisions/` and want agents to see which code implements which decision.
- You are onboarding a new contributor or agent and want file-level commentary to orient them before they start opening 400-line files.
- You have budget for a few cents per full refresh on frontier cloud models, or you are already running a local model server (Ollama, llama.cpp, LM Studio, vLLM) and want zero-cost commentary.
- You are comfortable treating the output as advisory, machine-authored prose. `synrepo` never silently promotes synthesis output into the structural graph.

Leave it off if none of the above apply, or if you want to keep `.synrepo/` fully offline and deterministic.

### Rough cost per full refresh

Ballpark for a 500-symbol repo, a full commentary pass with a frontier model. Your actual spend is always visible in the Health tab and `.synrepo/state/synthesis-totals.json`:

- **Anthropic / OpenAI / Gemini / OpenRouter** — a few cents per pass on frontier models, depending on the model you choose. Rate tables live in `src/pipeline/synthesis/pricing.rs` and are dated (`LAST_UPDATED` constant). Unknown `(provider, model)` pairs record `usd_cost: null` rather than guess.
- **Local (Ollama, llama.cpp, LM Studio, vLLM)** — no API cost. Quality depends on the model you pulled. Some OpenAI-compatible local servers do not return usage fields in their response; `synrepo` detects this and marks those calls `estimated` end-to-end so the dashboard never implies precision it does not have.

### How we track usage

Every synthesis call writes one JSON record to `.synrepo/state/synthesis-log.jsonl` (append-only) and updates a small aggregates snapshot at `.synrepo/state/synthesis-totals.json`. Both files are per-repo and local-only. Fields captured: call id, timestamp, provider, model, target node, duration, input + output tokens, usage source (`reported` or `estimated`), computed USD cost (nullable), outcome.

To inspect: read the JSONL directly, or open the TUI dashboard — synthesis calls stream into the Live tab and the Health tab surfaces lifetime totals. To reset: `synrepo sync --reset-synthesis-totals` rotates the JSONL with a timestamp suffix and zeros the totals snapshot. Pricing has a `last_updated` date surfaced in the Health block so stale rates do not silently under-report cost.

## Supported Languages

Structural extraction is wired for five languages. Parser output (symbols, call sites, imports) lands in the graph; stage-4 resolution promotes call and import references into cross-file edges where the language's contract allows.

| Language | Extensions | Symbols extracted | Signature + docs | Import resolution (stage 4) |
|---|---|---|---|---|
| Rust | `.rs` | `fn`, `struct`, `enum`, `trait`, `type`, `mod`, `const`, `static`, `impl` methods | Yes (`///` line comments) | Phase-1: `use` paths not resolved to files |
| Python | `.py` | `def`, `class`, methods, nested defs | Yes (docstring) | Dotted imports resolve to `a/b.py` |
| TypeScript | `.ts` | `function`, `class`, methods, `interface`, `type` alias | Yes (JSDoc `/** */`) | Relative paths resolve to target files |
| TSX | `.tsx` | same as TypeScript, plus JSX-bearing components | Yes (JSDoc `/** */`) | Relative paths resolve to target files |
| Go | `.go` | `func`, methods, `struct`, `interface` | No | Phase-1: imports not resolved to files |

Markdown concept extraction runs separately on the directories in `concept_directories` (defaults: `docs/concepts/`, `docs/adr/`, `docs/decisions/`).

Adding a new language requires a `tree-sitter-<lang>` grammar crate and co-located updates across the `Language` enum (query strings, extension map, pattern-index → `SymbolKind` table) plus a registered parser fixture. The parser test suite fails loudly if any surface is missed.

## Quick Start

The cleanest workflow to get `synrepo` running is:

1.  **Install synrepo**: See [Installation](#installation) above.
2.  **Run setup**: In your repository root, run:
    ```bash
    synrepo setup            # launches the interactive TUI wizard (mode + agent + synthesis)
    synrepo setup <agent>    # scripted, non-interactive flow for a known agent
    ```
    Two support tiers:
    - **Automated** (writes the shim *and* registers the project-scoped MCP server): `claude`, `codex`, `open-code`.
    - **Shim-only** (writes the instruction file; wire `synrepo mcp --repo .` into the agent's own config by hand): `cursor`, `copilot`, `windsurf`, `generic`, `gemini`, `goose`, `kiro`, `qwen`, `junie`, `roo`, `tabnine`, `trae`.

    `setup` always runs `init` and writes the client-specific instructions; it only registers the project-scoped MCP server for automated-tier agents. Bare `synrepo` on an uninitialized repo opens the same wizard automatically.
3.  **Use the agent**: Your agent (e.g., Claude Code, Cursor) will now load synrepo context via MCP.
4.  **Watch (Optional)**: If you want background refresh as you edit:
    ```bash
    synrepo watch --daemon
    ```

For low-level inspection:
```bash
synrepo status              # operational health: mode, counts, last reconcile, lock, export freshness
synrepo search "query"      # lexical search via the syntext index
synrepo graph stats         # node/edge counts
synrepo node <node_id>      # dump a node's metadata as JSON (e.g. file_0000000000000042)
synrepo check               # read-only drift report across repair surfaces
synrepo change-risk <target>  # risk assessment for a file or symbol
```

## How synrepo compares

synrepo is aimed at a different problem than most "agent memory" tools. The goal is not just to remember past sessions or store notes. The goal is to compile a repository into a durable, inspectable model that agents can query safely and incrementally.

### Summary

| Project | Primary problem it solves | Core model | Best fit | Where synrepo is stronger | Where synrepo is weaker |
|---|---|---|---|---|---|
| [synrepo](https://github.com/whit3rabbit/synrepo) | Repository understanding for coding agents | Canonical structural graph + separate overlay + agent-facing surfaces | Agents that need codebase structure, provenance, freshness, and targeted context | Stronger repo-native structure, clearer separation between canonical facts and generated overlay, better fit for repair/watch/reconcile workflows | More complex product story, more moving parts, and harder to explain than a simple memory tool |
| [memvid](https://github.com/memvid/memvid) | Portable long-term AI memory | Single-file memory layer with packaged data, embeddings, search structure, and metadata | Users who want portable memory, offline usage, and simple deployment | synrepo is stronger for codebase intelligence, structural relationships, and repo-aware context instead of generic memory retrieval | memvid is stronger on portability, simplicity, and the "one file" story |
| [claude-mem](https://github.com/thedotmack/claude-mem) | Session continuity for coding agents | Captured observations, summaries, and retrieval across sessions | Users who want automatic session memory with minimal workflow changes | synrepo is stronger when the source of truth should be the repository itself, not prior transcripts or session summaries | claude-mem is stronger on automatic workflow integration and immediate usefulness for session carry-over |
| [beads](https://github.com/gastownhall/beads) | Long-horizon task and issue coordination for agents | Dependency-aware graph issue tracker | Teams coordinating work, blockers, and multi-step agent execution | synrepo is stronger when the main problem is understanding the codebase rather than tracking work items | beads is stronger on task workflow clarity, agent operating doctrine, and explicit work-tracking UX |

### Honest positioning

These projects overlap, but they are not doing the same job.

- **memvid** is primarily a **portable memory system**
- **claude-mem** is primarily a **session memory system**
- **beads** is primarily a **task/work memory system**
- **synrepo** is primarily a **repository understanding system**

If the problem is **"help the agent remember prior sessions"**, tools like claude-mem have the cleaner story.

If the problem is **"give the agent portable long-term memory in a very easy-to-deploy package"**, memvid has the cleaner story.

If the problem is **"help agents coordinate tasks, blockers, and long-running work"**, beads has the cleaner story.

If the problem is **"help agents understand the repository itself with durable structure and inspectable context surfaces"**, synrepo has the stronger thesis.

### What synrepo is trying to do differently

synrepo is built around the idea that code understanding should not depend on replaying chat logs or stuffing everything into a generic memory store.

Instead, the intended direction is:

- compile the repository into a durable structural model
- separate canonical graph facts from generated overlay data
- expose agent-facing context through explicit surfaces rather than opaque retrieval alone
- support freshness, repair, reconcile, and watch workflows so the model can stay trustworthy over time

That makes synrepo less like a memory plugin and more like **infrastructure for code-aware agents**.

### Where synrepo is better

synrepo should be the better choice when you care about:

- file, symbol, concept, and edge-level repository structure
- durable provenance and inspectability
- separating hard facts from generated commentary
- repair and reconcile workflows
- agent-facing context surfaces that are repo-native instead of transcript-native
- using memory as a thin overlay on top of code intelligence, not as the primary source of truth

### Where synrepo is not better

synrepo is **not** the best fit when you mainly want:

- a simple portable memory file
- automatic session carry-over with minimal setup
- a task tracker for long-horizon execution
- the shortest path to "my agent remembers things now"

Those tools are solving a narrower problem, and that narrower scope is often a strength.

Design and architecture details live in [`docs/FOUNDATION.md`](docs/FOUNDATION.md) and [`docs/FOUNDATION-SPEC.md`](docs/FOUNDATION-SPEC.md).

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
