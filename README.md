# synrepo

[![CI](https://github.com/whit3rabbit/synrepo/actions/workflows/ci.yml/badge.svg)](https://github.com/whit3rabbit/synrepo/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/synrepo.svg)](https://crates.io/crates/synrepo)
[![Built With Ratatui](https://img.shields.io/badge/Built_With_Ratatui-000?logo=ratatui&logoColor=fff)](https://ratatui.rs/)

> WIP: `synrepo` is an in-progress context compiler for AI coding agents. It builds a deterministic local index and graph for a repository so agents can search, inspect structure, and make better edits with less blind file reading.

`synrepo` is a Rust workspace with a CLI and an MCP server. The project is built around a few hard boundaries: parser-observed facts live in the graph, machine-authored output belongs in a separate overlay, and the user-facing product is small task-shaped context instead of dumping large files into prompts.

**synrepo is not a session-memory tool or a task tracker. It is a context compiler for coding agents that need a durable, inspectable understanding of the repository itself.**

## What Exists Today

- CLI commands for `init`, `status`, `reconcile`, `check`, `sync`, `search`, `graph`, and `node`
- A persisted `.synrepo/` workspace with lexical index, graph store, config, and operational state
- Structural extraction for files, symbols, markdown concepts, and some cross-file edges
- An MCP server (`synrepo mcp`) for agent-facing repository context

## Quick Start

The cleanest workflow to get `synrepo` running is:

1.  **Install the binary**: Ensure `synrepo` is in your PATH.
2.  **Run setup**: In your repository root, run:
    ```bash
    synrepo setup claude    # or cursor, codex, opencode, etc.
    ```
    This runs `init`, writes client-specific instructions, and registers the project-scoped MCP server where possible.
3.  **Use the agent**: Your agent (e.g., Claude Code, Cursor) will now load synrepo context via MCP.
4.  **Watch (Optional)**: If you want background refresh as you edit:
    ```bash
    synrepo watch --daemon
    ```

For low-level inspection:
```bash
synrepo status
synrepo search "query"
synrepo graph stats
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

That distinction matters.

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
