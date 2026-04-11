## Why

The repo already has the Phase 0 substrate shape in code, but the actual behavior is still mostly stubbed: discovery and classification are TODOs, search is just a thin wrapper around `syntext`, and the durable contract does not yet say what must happen on ugly repositories. This change turns the lexical substrate from a placeholder into a real deterministic base layer the graph can build on.

## What Changes

- Define the first real Phase 0 lexical substrate behavior for discovery, classification, indexing, and exact search.
- Lock the file-handling rules for supported code, indexed-only text, markdown, notebooks, skipped binaries, and redacted content.
- Define how `.gitignore`, redaction globs, size caps, encoding checks, and LFS pointer detection affect discovery and indexing.
- Define index build, open, and rebuild behavior for the `synrepo init` and `synrepo search` flows.
- Add substrate tests and validation for classification, skip reasons, and exact-search behavior.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `substrate`: sharpen discovery, classification, encoding, and exact-search guarantees into implementable Phase 0 behavior

## Impact

- Affects the substrate and discovery implementation in `src/substrate/`
- Affects the initial compile/bootstrap path in the CLI surface (`src/bin/cli.rs`, `src/bin/cli_support/`)
- Adds or updates tests around discovery, file classification, and lexical search
- Does not change graph, overlay, card, or MCP behavior directly
