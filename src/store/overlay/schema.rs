//! Schema bootstrap for the overlay SQLite database.
//!
//! The overlay database lives at `.synrepo/overlay/overlay.db` and is
//! physically separate from the canonical graph store at
//! `.synrepo/graph/nodes.db`. No graph tables (`files`, `symbols`,
//! `concepts`, `edges`) are ever created here.
//!
//! ## Versioning
//!
//! The overlay schema carries a version number stored in the `meta` table:
//! - `v1`: commentary-only (commentary table + index)
//! - `v2`: commentary + cross-links (adds `cross_links`, `cross_link_audit`
//!   tables)
//! - `v3`: agent notes (adds `agent_notes`, `agent_note_transitions`,
//!   and `agent_note_links`)
//!
//! Migrations from v1 to v2 are additive and non-destructive: they run
//! `CREATE TABLE IF NOT EXISTS` for the new tables and bump the stored
//! version. No existing rows move.

use rusqlite::Connection;

/// Current overlay schema version shipped by this binary.
pub(super) const CURRENT_SCHEMA_VERSION: u32 = 3;

pub(super) fn init_schema(conn: &Connection) -> crate::Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;

        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS commentary (
            id INTEGER PRIMARY KEY,
            node_id TEXT NOT NULL UNIQUE,
            text TEXT NOT NULL,
            source_content_hash TEXT NOT NULL,
            pass_id TEXT NOT NULL,
            model_identity TEXT NOT NULL,
            generated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_commentary_node_id ON commentary(node_id);
        CREATE INDEX IF NOT EXISTS idx_commentary_generated_at ON commentary(generated_at);

        -- Cross-link candidates. Keyed on (from_node, to_node, kind) so a
        -- single (source, target, relationship) triple has at most one active
        -- candidate row. Audit history lives in `cross_link_audit`.
        CREATE TABLE IF NOT EXISTS cross_links (
            id INTEGER PRIMARY KEY,
            from_node TEXT NOT NULL,
            to_node TEXT NOT NULL,
            kind TEXT NOT NULL,
            epistemic TEXT NOT NULL,
            source_spans_json TEXT NOT NULL,
            target_spans_json TEXT NOT NULL,
            from_content_hash TEXT NOT NULL,
            to_content_hash TEXT NOT NULL,
            confidence_score REAL NOT NULL,
            confidence_tier TEXT NOT NULL,
            rationale TEXT,
            pass_id TEXT NOT NULL,
            model_identity TEXT NOT NULL,
            generated_at TEXT NOT NULL,
            state TEXT NOT NULL DEFAULT 'active',
            reviewer TEXT,
            promoted_at TEXT,
            graph_edge_id TEXT,
            UNIQUE(from_node, to_node, kind)
        );

        CREATE INDEX IF NOT EXISTS idx_cross_links_from_node ON cross_links(from_node);
        CREATE INDEX IF NOT EXISTS idx_cross_links_to_node ON cross_links(to_node);
        CREATE INDEX IF NOT EXISTS idx_cross_links_tier ON cross_links(confidence_tier);
        CREATE INDEX IF NOT EXISTS idx_cross_links_state ON cross_links(state);

        -- Immutable audit log. One row per lifecycle event. Rows are never
        -- updated or deleted — deletion of a candidate from `cross_links`
        -- leaves its audit rows intact.
        CREATE TABLE IF NOT EXISTS cross_link_audit (
            id INTEGER PRIMARY KEY,
            from_node TEXT NOT NULL,
            to_node TEXT NOT NULL,
            kind TEXT NOT NULL,
            event_kind TEXT NOT NULL,
            reviewer TEXT,
            previous_tier TEXT,
            new_tier TEXT,
            reason TEXT,
            pass_id TEXT NOT NULL,
            model_identity TEXT NOT NULL,
            event_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_cross_link_audit_endpoints
            ON cross_link_audit(from_node, to_node, kind);

        CREATE TABLE IF NOT EXISTS agent_notes (
            note_id TEXT PRIMARY KEY,
            target_kind TEXT NOT NULL,
            target_id TEXT NOT NULL,
            claim TEXT NOT NULL,
            evidence_json TEXT NOT NULL,
            created_by TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            confidence TEXT NOT NULL,
            status TEXT NOT NULL,
            source_hashes_json TEXT NOT NULL,
            graph_revision INTEGER,
            expires_on_drift INTEGER NOT NULL,
            supersedes_json TEXT NOT NULL,
            superseded_by TEXT,
            verified_at TEXT,
            verified_by TEXT,
            invalidated_by TEXT,
            source_store TEXT NOT NULL DEFAULT 'overlay',
            advisory INTEGER NOT NULL DEFAULT 1
        );

        CREATE INDEX IF NOT EXISTS idx_agent_notes_target
            ON agent_notes(target_kind, target_id);
        CREATE INDEX IF NOT EXISTS idx_agent_notes_status
            ON agent_notes(status);
        CREATE INDEX IF NOT EXISTS idx_agent_notes_updated_at
            ON agent_notes(updated_at);

        CREATE TABLE IF NOT EXISTS agent_note_transitions (
            id INTEGER PRIMARY KEY,
            note_id TEXT NOT NULL,
            action TEXT NOT NULL,
            previous_status TEXT,
            new_status TEXT NOT NULL,
            actor TEXT NOT NULL,
            reason TEXT,
            related_note TEXT,
            happened_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_agent_note_transitions_note
            ON agent_note_transitions(note_id);

        CREATE TABLE IF NOT EXISTS agent_note_links (
            id INTEGER PRIMARY KEY,
            from_note TEXT NOT NULL,
            to_note TEXT NOT NULL,
            actor TEXT NOT NULL,
            created_at TEXT NOT NULL,
            UNIQUE(from_note, to_note)
        );

        CREATE INDEX IF NOT EXISTS idx_agent_note_links_from
            ON agent_note_links(from_note);
        CREATE INDEX IF NOT EXISTS idx_agent_note_links_to
            ON agent_note_links(to_note);
        ",
    )?;

    // Record the current schema version after tables exist so a freshly
    // created DB and a v1→v2 migration both land at v2.
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![CURRENT_SCHEMA_VERSION.to_string()],
    )?;

    Ok(())
}

/// Read the stored overlay schema version. Returns `None` if the `meta` table
/// is absent (fresh DB created before this module shipped).
#[allow(dead_code)]
pub(super) fn read_schema_version(conn: &Connection) -> crate::Result<Option<u32>> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='meta'",
        [],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Ok(None);
    }
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .ok();
    match value {
        Some(s) => Ok(s.parse::<u32>().ok()),
        None => Ok(None),
    }
}
