## Why

The repo already commits to a concrete `.synrepo/` layout and config file shape, but the compatibility rules are still implied instead of owned by an active change. `synrepo init` creates runtime directories and writes `config.toml`, the docs define retention and compaction expectations, and the new durable spec says migrations and rebuilds matter, yet no change currently states what is canonical, disposable, compatibility-sensitive, or upgrade-triggering.

This change is opened early to lock the storage contract while the bootstrap and substrate shape is still small. It does not by itself move storage or ops implementation ahead of the roadmap milestones that establish the underlying stores and operational surfaces.

## What Changes

- Tighten the durable storage and ops specs so they name current store classes, per-store compatibility actions, and compatibility-sensitive config groupings.
- Add a thin shared runtime compatibility layer that owns store-format expectations, config fingerprints, and compatibility decisions outside bootstrap.
- Lock rebuild versus migration behavior for index, graph, overlay, embeddings, cache, and state stores.
- Define retention and maintenance expectations that later operational commands can implement without inventing policy.
- Add implementation tasks and validation expectations for persisted compatibility metadata, compatibility checks, CLI guidance, and maintenance-facing semantics.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `storage-and-compatibility`: sharpen `.synrepo/` responsibilities, migration policy, retention, rebuild behavior, and config compatibility semantics
- `watch-and-ops`: clarify how operational maintenance surfaces apply storage retention, cleanup, rebuild, and migration rules

## Impact

- Affects `.synrepo/` layout ownership and config semantics in the CLI surface (`src/bin/cli.rs`, `src/bin/cli_support/`) and `src/config.rs`
- Adds a shared compatibility policy surface under `src/store/` for bootstrap, substrate, and later store backends to reuse
- Affects future store backends and maintenance commands under `src/store/` and operational flows
- Provides contract guardrails for later `watch-reconcile-v1`, export work, and migration behavior
- Does not itself implement full maintenance commands or daemon behavior
