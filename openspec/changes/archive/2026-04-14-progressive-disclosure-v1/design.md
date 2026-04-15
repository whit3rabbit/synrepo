## Context

Two surfaces ship in this change:

1. **`synrepo_recent_activity` MCP tool** — a bounded lane over synrepo's own operational events. The ROADMAP.md §11.2 Phase 5 spec says data is "already persisted in `.synrepo/state/` and the overlay store." The actual persistence is:
   - `reconcile`: `.synrepo/state/reconcile-state.json` (`ReconcileState` struct) — stores only the **most recent** reconcile outcome. No history exists.
   - `repair`: `.synrepo/state/repair-log.jsonl` — append-only JSONL, each line a `ResolutionLogEntry` (timestamp, surfaces, findings, actions, outcome).
   - `cross_link`: `cross_link_audit` SQLite table in `.synrepo/overlay/overlay.db` — immutable audit trail with `event_kind`, `event_at`, endpoint node IDs.
   - `overlay_refresh`: `commentary` SQLite table — `generated_at` per entry; no explicit refresh-log.
   - `hotspot`: Git intelligence index (`GitHistoryIndex`), in-memory only; must be built on demand per-request.

2. **Progressive-disclosure doc pass** — no new code. Adds a formal requirement to `openspec/specs/cards/spec.md` and updates `skill/SKILL.md` with explicit escalation guidance.

## Goals / Non-Goals

**Goals:**
- Ship `synrepo_recent_activity` MCP tool with `kinds`, `limit` (default 20, max 200), and `since` filters; refuse unbounded lookback.
- Ship `synrepo status --recent` flag using the same bounded read logic.
- Add progressive-disclosure protocol requirement to the cards spec and agent skill.
- Zero new storage: read from existing files and tables only.

**Non-Goals:**
- No reconcile history log. The tool returns the single persisted reconcile state as one entry when `kinds` includes `reconcile`. Callers see at most one reconcile event.
- No `overlay_refresh` history beyond what the commentary table already stores (`generated_at` per node). The tool aggregates recent commentary rows by timestamp.
- No cross-session agent-interaction log or prompt-capture. The tool is strictly an operational events surface.
- No hotspot persistence. Hotspots are computed from the in-memory Git intelligence index built from `GitHistoryIndex::build`; they are expensive and skipped when git is unavailable.

## Decisions

### D1: Reconcile history is one entry
`ReconcileState` persists only the last outcome. Adding a rolling reconcile log is out of scope for this change. The tool returns at most one `reconcile` event. This is accurate and avoids scope creep — the data that would power a history log is `repair-log.jsonl`, not `reconcile-state.json`.

### D2: Overlay refresh sourced from commentary table timestamps
The commentary table has `generated_at` per node. The tool reads the N most-recent `generated_at` rows and returns them as `overlay_refresh` events. This is approximate (no refresh-specific event kind in the schema) but truthful — it surfaces the actual last-generated timestamp per node.

Alternative considered: add an `overlay_refresh_log` table to the overlay store. Deferred — the commentary table timestamps are sufficient for Phase 5 scope.

### D3: Hotspot kind computed on demand, skipped when git unavailable
`GitHistoryIndex::build` is called per-request when `kinds` includes `hotspot`. If the repo has no git history (or `GitCache` returns `Unavailable`), hotspot entries are an empty list with a `state: "unavailable"` label rather than an error. This matches the existing degraded-history behavior in git-intelligence.

Alternative: pre-compute and persist hotspot ranks during reconcile. Deferred — adds schema complexity with no current consumer.

### D4: MCP handler reads files directly, not through GraphCardCompiler
`synrepo_recent_activity` reads `.synrepo/state/` files and the overlay SQLite DB directly. It does not go through `GraphCardCompiler` (which is graph-oriented). This keeps the handler simple and avoids threading a card compiler through an operational query.

The overlay store's `SqliteOverlayStore` already exposes the connection; the handler will open it read-only via `SqliteOverlayStore::open_read_only` (or the existing `open` path in read-only mode).

### D5: `synrepo status --recent` is a flag on the existing status command
Not a subcommand. It reuses the bounded-read logic from the MCP handler via a shared internal function `read_recent_activity(synrepo_dir, kinds, limit, since)` that both the CLI flag and MCP handler call.

### D6: Progressive-disclosure pass is documentation only
`skill/SKILL.md` update + `openspec/specs/cards/spec.md` delta. No code changes. The three-tier budget behavior is already implemented; this pass encodes the escalation intent as a first-class contract so it survives future spec changes.

## Risks / Trade-offs

**[Hotspot on-demand cost]** → Building `GitHistoryIndex` per MCP call is O(git history depth). Mitigation: the GitCache on `GraphCardCompiler` is already bounded by `git_commit_depth` (default 500). The handler builds from the same cache if a compiler is available, or skips hotspot if not.

**[Overlay store contention]** → The handler opens the overlay SQLite DB for reading while a reconcile may be writing. Mitigation: `busy_timeout = 5000` is already set on the overlay store; WAL mode allows concurrent reads.

**[reconcile single-entry limitation]** → Callers expecting a time-series of reconcile events will see only one. Mitigation: the tool response labels the entry with `note: "single_entry"` so callers understand this is not a log.

**[commentary table scan cost]** → Sorting all commentary rows by `generated_at` is O(N) without an index on that column. Mitigation: add `CREATE INDEX IF NOT EXISTS idx_commentary_generated_at ON commentary(generated_at)` during tool initialization; the column already exists in the schema.

## Migration Plan

No schema migrations required. The overlay schema already has all needed tables and columns. The `commentary` table needs one new index (added idempotently). All `.synrepo/state/` files are existing JSON/JSONL formats.

## Open Questions

- None blocking. The scope is narrow and all data sources are stable.
