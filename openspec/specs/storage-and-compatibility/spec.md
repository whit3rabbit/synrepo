## Purpose
Define the durable contract for `.synrepo/` storage layout, compatibility-sensitive configuration, migrations, rebuild behavior, and retention boundaries.

## Requirements

### Requirement: Define `.synrepo/` store responsibilities
synrepo SHALL define the purpose, durability class, and compatibility owner of the major `.synrepo/` stores and distinguish compatibility-sensitive state from disposable caches and regenerable artifacts.

#### Scenario: Inspect on-disk state
- **WHEN** a contributor needs to understand what lives under `.synrepo/`
- **THEN** the contract identifies which stores are canonical, supplemental, rebuildable, disposable, or ephemeral
- **AND** it distinguishes what may be deleted and rebuilt from what must be migrated or preserved
- **AND** it defines the default compatibility action for each current store, including `graph`, `overlay`, `index`, `embeddings`, `cache/llm-responses`, and `state`
- **AND** it recognises synthesis telemetry files `state/synthesis-log.jsonl` (append-only per-call records) and `state/synthesis-totals.json` (aggregates snapshot) as operational state that is disposable and rotatable without affecting canonical graph or overlay data

### Requirement: Define migration and rebuild policy
synrepo SHALL define when schema or format changes require in-place migration, full rebuild, cache invalidation, clear-and-recreate, safe continue, or explicit user action.

#### Scenario: Upgrade synrepo across a storage change
- **WHEN** a new synrepo version changes a persisted store format or compatibility-sensitive config field
- **THEN** the contract identifies whether the change continues safely, rebuilds, invalidates, clears and recreates, migrates, or refuses to proceed
- **AND** the outcome is deterministic instead of being left to implementation guesswork
- **AND** canonical stores are never silently deleted to satisfy compatibility

### Requirement: Define the upgrade command as the explicit migration entry point
synrepo SHALL treat `synrepo upgrade` as the single explicit entry point for applying compatibility actions to `.synrepo/` stores after a binary version change. The upgrade command SHALL reuse the compatibility evaluator to determine the required action per store and SHALL require `--apply` before executing any mutating actions.

#### Scenario: Inspect stores after a binary update
- **WHEN** a user runs `synrepo upgrade` without `--apply`
- **THEN** the command reads each store's recorded schema version, applies the compatibility evaluator, and prints a plan showing each store, its version, and the required action
- **AND** no stores are mutated
- **AND** the command exits with a non-zero code if any store requires action (to support scripted upgrade pipelines)

#### Scenario: Apply compatibility actions
- **WHEN** a user runs `synrepo upgrade --apply`
- **THEN** each store receives its required compatibility action (continue / rebuild / invalidate / clear-and-recreate / migrate)
- **AND** `block` actions prevent the upgrade from continuing and emit a clear error with manual steps
- **AND** the command exits zero if all stores reach a usable state

#### Scenario: Detect version skew at startup without upgrade
- **WHEN** synrepo starts and detects that a store's version is outside the supported range without the user running `upgrade`
- **THEN** the startup path applies the compatibility action defined for that store (typically `block` for unsupported-newer versions)
- **AND** the error message includes the suggested `synrepo upgrade` invocation
- **AND** canonical stores are never silently deleted on startup to resolve version skew

### Requirement: Define compact-specific maintenance actions
synrepo SHALL define compact-specific maintenance actions that extend the existing compatibility-driven maintenance model. These actions target overlay commentary retention, cross-link audit summarization, repair-log rotation, lexical index rebuild, and SQLite WAL checkpointing.

#### Scenario: Plan a compact pass
- **WHEN** a user runs `synrepo compact` without `--apply`
- **THEN** the maintenance planner evaluates each store against the selected policy and produces a plan listing compactable commentary entries, summarizable cross-link audit rows, rotatable repair-log entries, and whether a WAL checkpoint or index rebuild is warranted
- **AND** no stores are mutated
- **AND** the plan includes estimated row counts for each action

#### Scenario: Execute a compact pass
- **WHEN** a user runs `synrepo compact --apply`
- **THEN** each planned action is executed in order: overlay commentary compaction, cross-link audit summarization, repair-log rotation, index rebuild (if warranted), and WAL checkpoint on both databases
- **AND** the command exits zero if all actions succeed
- **AND** a summary of rows affected per action is printed

### Requirement: Define compact policy presets
synrepo SHALL define three compact policy presets (`default`, `aggressive`, `audit-heavy`) with hard-coded retention thresholds. Policy selection SHALL be explicit via `--policy` flag, defaulting to `default`.

#### Scenario: Apply default policy
- **WHEN** a compact pass runs with `default` policy
- **THEN** stale commentary entries older than 30 days are dropped, active commentary is retained, cross-link audit rows for promoted or rejected candidates older than 90 days are summarized into counts, repair-log entries older than 30 days are summarized into a header line, and a WAL checkpoint runs on both databases

#### Scenario: Apply aggressive policy
- **WHEN** a compact pass runs with `aggressive` policy
- **THEN** stale commentary entries older than 7 days are dropped, cross-link audit rows older than 30 days are summarized, and repair-log entries older than 7 days are summarized

#### Scenario: Apply audit-heavy policy
- **WHEN** a compact pass runs with `audit-heavy` policy
- **THEN** stale commentary entries older than 90 days are dropped, cross-link audit rows are retained in full for 180 days before summarization, and repair-log entries older than 90 days are summarized

### Requirement: Compaction SHALL NOT touch canonical graph rows
synrepo SHALL enforce that the compact command never modifies or deletes rows in the canonical graph store (`nodes.db`). This includes nodes, edges, and provenance records.

#### Scenario: Verify graph integrity after compaction
- **WHEN** a compact pass completes
- **THEN** all canonical graph rows (FileNode, SymbolNode, ConceptNode, Edge, Provenance) are unchanged
- **AND** a test exists that asserts row counts and content are identical before and after compaction

### Requirement: Define overlay store compaction trait methods
synrepo SHALL extend the `OverlayStore` trait with methods for querying compactable row counts and executing commentary and cross-link compaction according to the supplied policy.

#### Scenario: Query compactable commentary stats
- **WHEN** the maintenance planner needs compactable commentary statistics
- **THEN** the overlay store returns counts of stale entries within and beyond the policy's retention window, grouped by staleness age

#### Scenario: Execute commentary compaction
- **WHEN** a compact pass executes commentary compaction
- **THEN** the overlay store drops stale commentary entries older than the policy threshold and retains all active entries
- **AND** the returned summary includes the count of entries dropped

#### Scenario: Execute cross-link audit summarization
- **WHEN** a compact pass executes cross-link audit compaction
- **THEN** the overlay store summarizes promoted and rejected audit rows older than the policy threshold into aggregate counts and drops the individual rows
- **AND** active candidates are never summarized or dropped by compaction
- **AND** the returned summary includes the count of rows summarized and dropped

### Requirement: Define compatibility-sensitive configuration
synrepo SHALL define which configuration fields affect on-disk compatibility, indexing semantics, graph semantics, or synthesis behavior strongly enough to require rebuild, invalidation, warning, or migration decisions.

#### Scenario: Change a compatibility-sensitive setting
- **WHEN** a user modifies a config field that changes indexing or storage semantics
- **THEN** synrepo can determine whether to rebuild, invalidate, migrate, warn, or continue
- **AND** the decision is governed by the compatibility contract rather than hidden implementation detail

#### Scenario: Group current config fields by compatibility impact
- **WHEN** a contributor inspects the compatibility-sensitive config contract
- **THEN** `roots`, `max_file_size_bytes`, and `redact_globs` are classified as discovery and index compatibility inputs
- **AND** `concept_directories` and `git_commit_depth` are classified as future graph or overlay compatibility inputs that may require guidance before those stores are fully implemented
- **AND** `mode` is treated as operational configuration, not a persisted-store compatibility trigger
