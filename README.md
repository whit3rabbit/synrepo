# synrepo

[![CI](https://github.com/whit3rabbit/synrepo/actions/workflows/ci.yml/badge.svg)](https://github.com/whit3rabbit/synrepo/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/synrepo.svg)](https://crates.io/crates/synrepo)

> WIP: `synrepo` is an in-progress context compiler for AI coding agents. It builds a deterministic local index and graph for a repository so agents can search, inspect structure, and make better edits with less blind file reading.

`synrepo` is a Rust workspace with a CLI and an MCP server. The project is built around a few hard boundaries: parser-observed facts live in the graph, machine-authored output belongs in a separate overlay, and the user-facing product is small task-shaped context instead of dumping large files into prompts.

## What Exists Today

- CLI commands for `init`, `status`, `reconcile`, `check`, `sync`, `search`, `graph`, and `node`
- A persisted `.synrepo/` workspace with lexical index, graph store, config, and operational state
- Structural extraction for files, symbols, markdown concepts, and some cross-file edges
- An MCP server (`synrepo mcp`) for agent-facing repository context

## Quick Start

```bash
cargo run -- init
cargo run -- status
cargo run -- search "query"
cargo run -- graph stats
```

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
