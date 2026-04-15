## Why

`SymbolCard.last_change` currently reports the most recent commit touching the containing file, labeled `granularity: "file"`. This is a known approximation: a symbol that has not changed in months inherits the recency signal of its file, which degrades the agent's ability to prioritize genuinely stale code versus actively maintained code. The `body_hash` field on `SymbolNode` already changes deterministically when a symbol's body is rewritten, making true symbol-level tracking straightforward to wire.

## What Changes

- Add `first_seen_rev` and `last_modified_rev` fields to `SymbolNode` in the graph store, populated during stage 5 (git mining) by correlating `body_hash` transitions against sampled commit history.
- Upgrade `symbol_last_change_from_insights` to prefer the symbol-scoped revision when available, falling back to the file-level projection unchanged.
- Change `SymbolCard.last_change.granularity` from `"file"` to `"symbol"` when the symbol-scoped revision is used.
- The `last_change` shape (`revision`, `author_name`, `committed_at_unix`, `summary`, `granularity`) is unchanged. Consumers that read the existing fields continue to function without modification (non-breaking, as the cards spec already promises).

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `git-intelligence`: adds requirement for symbol-scoped revision tracking, defining how `first_seen_rev` and `last_modified_rev` are derived from `body_hash` transitions in sampled history
- `cards`: updates the `SymbolCard.last_change` scenario to reflect that `granularity: "symbol"` is now the default when symbol-scoped data is available, with `"file"` as the documented fallback

## Impact

- **Graph store schema**: `SymbolNode` gains two optional string columns (`first_seen_rev`, `last_modified_rev`). Existing rows have `NULL` for both until the next structural compile populates them. No migration required beyond the additive column addition.
- **Stage 5 (git mining)**: extended to diff per-symbol `body_hash` across sampled commits within the existing `git_commit_depth` budget. No additional history walks.
- **Card compiler**: `symbol_last_change_from_insights` reads the new fields; no card shape changes.
- **SQLite compatibility**: additive columns only; `synrepo upgrade --apply` handles the schema extension for existing stores.
- **Export surface**: `SymbolCard.last_change.granularity` may now emit `"symbol"` instead of `"file"`. JSON consumers that branch on the `granularity` field gain a new value; consumers that ignore it are unaffected.
