# Saved Repo Lessons

This document defines the shipped user-facing design for explicit saved repo
context. It does not add automatic agent memory. The storage substrate is the
existing overlay agent-note model, so saved lessons stay advisory and separate
from canonical graph facts.

## Goal

Give agents and operators a small, reviewable way to save repo-specific lessons
such as "this module owns setup retries" or "do not edit generated branch-ref
snapshots" without turning synrepo into general personal memory.

Saved lessons must be:

- Repo-scoped, never global by default.
- Explicitly written by a user or an MCP client running with overlay writes.
- Provenance-tagged with actor, target, source hash when available, and graph
  revision when known.
- Freshness-aware: stale source hashes degrade recall labels instead of hiding
  drift.
- Advisory only. They can appear in cards, resume context, and note reads, but
  they never become graph nodes, graph edges, parser facts, or explain input.

## Public Surface

CLI names:

- `synrepo remember <claim> [--target <target>] [--target-kind repo|path|file|symbol|concept|test|card|note] [--ttl <duration>] [--evidence <text>]...`
- `synrepo recall <query> [--target <target>] [--target-kind <kind>] [--limit <n>] [--include-hidden]`
- `synrepo lessons [--target <target>] [--target-kind <kind>] [--limit <n>] [--include-hidden]`
- `synrepo forget <lesson-id> [--reason <text>]`
- `synrepo verify-lesson <lesson-id>`

MCP names:

- `synrepo_lesson_add`
- `synrepo_lesson_search`
- `synrepo_lesson_list`
- `synrepo_lesson_forget`
- `synrepo_lesson_verify`

These names are intentionally distinct from `synrepo_note_*`: lessons are a
simple operator-facing workflow over the note store, while notes remain the
lower-level overlay primitive.

## Stored Fields

Each saved lesson maps to an `AgentNote` row plus reserved evidence metadata.
There is no SQLite DDL change. The marker is
`{ "kind": "synrepo.lesson", "id": "v1" }`. TTL is stored as
`{ "kind": "synrepo.lesson.expires_at", "id": "<RFC3339 UTC>" }`.

Public lesson responses expose:

- `lesson_id`: stable note id returned to the caller.
- `claim`: bounded text, max 4000 characters.
- `target_kind`: `repo`, `path`, `file`, `symbol`, `concept`, `test`, `card`, or `note`.
- `target`: repo-relative path, node id, note id, card target, or `.`.
- `actor`: explicit CLI/MCP actor label.
- `created_at`, `updated_at`, optional `expires_at`.
- `source_hashes`: up to 32 graph/file source hashes.
- `graph_revision`: optional observed graph revision.
- `confidence`: `low`, `medium`, or `high`.
- `status`: `active`, `unverified`, `stale`, `superseded`, `forgotten`, or `invalid`.
- `freshness`: `fresh`, `stale`, `expired`, or `hidden`.
- `evidence`: up to 32 bounded evidence entries.

No prompt logs, chat transcripts, tool outputs, caller identity, credentials, or
unbounded source snippets are stored.

## Query Behavior

Recall is deterministic and bounded:

- Filter by repo first, then target when supplied.
- Exclude `forgotten`, `superseded`, `invalid`, and expired lessons by default.
- Prefer exact target matches, verified notes, fresh source hashes, and recent
  updates.
- Return at most 20 lessons per call and at most 4000 estimated response tokens.
- Include freshness labels and source-hash mismatch notes in every result.

When a saved lesson targets a graph node whose current source hash no longer
matches, recall returns the lesson as `stale` with a recommended verification
action. It must not silently refresh the lesson or promote stale text.

## Resume Context

`synrepo_resume_context(include_notes=true)` includes a compact
`saved_lessons` section alongside the existing `saved_notes` section. The
lesson section contains only:

- lesson id
- target
- lifecycle state
- freshness
- confidence
- updated timestamp
- short claim preview

Full lesson bodies remain available through the explicit list/search tools.

## Limits And Safety

- MCP lesson write tools are visible only under `synrepo mcp --allow-overlay-writes`.
- Read-only MCP defaults do not change.
- Expired lessons are omitted from default reads but remain auditable until
  overlay compaction policy removes them.
- Lessons must never be read by explain generation as source material.

## Acceptance Coverage

The implementation covers:

- TTL expiry hides default recall results.
- Source-hash drift labels a lesson stale.
- Recall stays bounded by count and token budget.
- Read-only MCP mode does not expose write tools.
- Lessons are stored only in the overlay database.
- Graph node and edge counts do not change after lesson writes.
