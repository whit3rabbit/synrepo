use std::{path::Path, str::FromStr};

use synrepo::{
    config::Config,
    overlay::{
        AgentNote, AgentNoteConfidence, AgentNoteQuery, AgentNoteSourceHash, AgentNoteTarget,
        AgentNoteTargetKind, OverlayStore,
    },
    pipeline::writer::{acquire_write_admission, map_lock_error},
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
};

const DEFAULT_NOTES_LIMIT: usize = 20;

#[allow(clippy::too_many_arguments)]
pub(crate) fn notes_add(
    repo_root: &Path,
    target_kind: &str,
    target: &str,
    claim: &str,
    created_by: &str,
    confidence: &str,
    evidence_json: Option<&str>,
    source_hashes_json: Option<&str>,
    graph_revision: Option<u64>,
    json_output: bool,
) -> anyhow::Result<()> {
    print!(
        "{}",
        notes_add_output(
            repo_root,
            target_kind,
            target,
            claim,
            created_by,
            confidence,
            evidence_json,
            source_hashes_json,
            graph_revision,
            json_output,
        )?
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn notes_add_output(
    repo_root: &Path,
    target_kind: &str,
    target: &str,
    claim: &str,
    created_by: &str,
    confidence: &str,
    evidence_json: Option<&str>,
    source_hashes_json: Option<&str>,
    graph_revision: Option<u64>,
    json_output: bool,
) -> anyhow::Result<String> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let _lock = acquire_write_admission(&synrepo_dir, "notes add")
        .map_err(|err| map_lock_error("notes add", err))?;
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay"))?;
    let mut note = build_note(
        repo_root,
        target_kind,
        target,
        claim,
        created_by,
        confidence,
        evidence_json,
        source_hashes_json,
        graph_revision,
    )?;
    fill_default_source_anchor(repo_root, &mut note);
    let note = overlay.insert_note(note)?;
    render_note_mutation("added", &note, json_output)
}

pub(crate) fn notes_list(
    repo_root: &Path,
    target_kind: Option<&str>,
    target: Option<&str>,
    limit: Option<usize>,
    include_all: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    print!(
        "{}",
        notes_list_output(
            repo_root,
            target_kind,
            target,
            limit,
            include_all,
            json_output
        )?
    );
    Ok(())
}

pub(crate) fn notes_list_output(
    repo_root: &Path,
    target_kind: Option<&str>,
    target: Option<&str>,
    limit: Option<usize>,
    include_all: bool,
    json_output: bool,
) -> anyhow::Result<String> {
    let overlay = open_existing_overlay(repo_root)?;
    let query = build_query(target_kind, target, limit, include_all)?;
    let notes = overlay.query_notes(query)?;
    render_notes(&notes, json_output)
}

pub(crate) fn notes_audit(
    repo_root: &Path,
    target_kind: Option<&str>,
    target: Option<&str>,
    limit: Option<usize>,
    json_output: bool,
) -> anyhow::Result<()> {
    print!(
        "{}",
        notes_list_output(repo_root, target_kind, target, limit, true, json_output)?
    );
    Ok(())
}

pub(crate) fn notes_link(
    repo_root: &Path,
    from_note: &str,
    to_note: &str,
    actor: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let _lock = acquire_write_admission(&synrepo_dir, "notes link")
        .map_err(|err| map_lock_error("notes link", err))?;
    let mut overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    overlay.link_note(from_note, to_note, actor)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "linked": true,
                "from_note": from_note,
                "to_note": to_note,
                "source_store": "overlay",
                "advisory": true
            }))?
        );
    } else {
        println!("Linked {from_note} -> {to_note}.");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn notes_supersede(
    repo_root: &Path,
    old_note: &str,
    target_kind: &str,
    target: &str,
    claim: &str,
    created_by: &str,
    confidence: &str,
    evidence_json: Option<&str>,
    source_hashes_json: Option<&str>,
    graph_revision: Option<u64>,
    json_output: bool,
) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let _lock = acquire_write_admission(&synrepo_dir, "notes supersede")
        .map_err(|err| map_lock_error("notes supersede", err))?;
    let mut overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    let mut replacement = build_note(
        repo_root,
        target_kind,
        target,
        claim,
        created_by,
        confidence,
        evidence_json,
        source_hashes_json,
        graph_revision,
    )?;
    fill_default_source_anchor(repo_root, &mut replacement);
    let note = overlay.supersede_note(old_note, replacement, created_by)?;
    print!(
        "{}",
        render_note_mutation("superseded", &note, json_output)?
    );
    Ok(())
}

pub(crate) fn notes_forget(
    repo_root: &Path,
    note_id: &str,
    actor: &str,
    reason: Option<&str>,
    json_output: bool,
) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let _lock = acquire_write_admission(&synrepo_dir, "notes forget")
        .map_err(|err| map_lock_error("notes forget", err))?;
    let mut overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    overlay.forget_note(note_id, actor, reason)?;
    render_simple_status("forgotten", note_id, json_output)
}

pub(crate) fn notes_verify(
    repo_root: &Path,
    note_id: &str,
    actor: &str,
    graph_revision: Option<u64>,
    json_output: bool,
) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let _lock = acquire_write_admission(&synrepo_dir, "notes verify")
        .map_err(|err| map_lock_error("notes verify", err))?;
    let mut overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    let note = overlay.verify_note(note_id, actor, graph_revision)?;
    print!("{}", render_note_mutation("verified", &note, json_output)?);
    Ok(())
}

fn open_existing_overlay(repo_root: &Path) -> anyhow::Result<SqliteOverlayStore> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))
        .map_err(|error| anyhow::anyhow!("Could not open overlay store: {error}"))
}

#[allow(clippy::too_many_arguments)]
fn build_note(
    _repo_root: &Path,
    target_kind: &str,
    target: &str,
    claim: &str,
    created_by: &str,
    confidence: &str,
    evidence_json: Option<&str>,
    source_hashes_json: Option<&str>,
    graph_revision: Option<u64>,
) -> anyhow::Result<AgentNote> {
    let target_kind = AgentNoteTargetKind::from_str(target_kind)?;
    let confidence = AgentNoteConfidence::from_str(confidence)?;
    let mut note = AgentNote::new(
        AgentNoteTarget {
            kind: target_kind,
            id: target.to_string(),
        },
        claim.to_string(),
        created_by.to_string(),
        confidence,
    );
    note.evidence = parse_json_array(evidence_json, "evidence-json")?;
    note.source_hashes = parse_json_array(source_hashes_json, "source-hashes-json")?;
    note.graph_revision = graph_revision;
    Ok(note)
}

fn parse_json_array<T: serde::de::DeserializeOwned>(
    raw: Option<&str>,
    name: &str,
) -> anyhow::Result<Vec<T>> {
    match raw {
        Some(value) => {
            serde_json::from_str(value).map_err(|err| anyhow::anyhow!("invalid --{name}: {err}"))
        }
        None => Ok(Vec::new()),
    }
}

fn fill_default_source_anchor(repo_root: &Path, note: &mut AgentNote) {
    if !note.source_hashes.is_empty() || note.target.kind != AgentNoteTargetKind::Path {
        return;
    }
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let Ok(graph) = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")) else {
        return;
    };
    let Ok(Some(file)) = graph.file_by_path(&note.target.id) else {
        return;
    };
    note.source_hashes.push(AgentNoteSourceHash {
        path: file.path,
        hash: file.content_hash,
        root_id: Some(file.root_id),
    });
}

fn build_query(
    target_kind: Option<&str>,
    target: Option<&str>,
    limit: Option<usize>,
    include_all: bool,
) -> anyhow::Result<AgentNoteQuery> {
    Ok(AgentNoteQuery {
        target_kind: target_kind.map(AgentNoteTargetKind::from_str).transpose()?,
        target_id: target.map(ToOwned::to_owned),
        include_forgotten: include_all,
        include_superseded: include_all,
        include_invalid: include_all,
        limit: limit.unwrap_or(DEFAULT_NOTES_LIMIT),
    })
}

fn render_notes(notes: &[AgentNote], json_output: bool) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let mut out = String::new();
    if json_output {
        writeln!(out, "{}", serde_json::to_string_pretty(notes)?).unwrap();
        return Ok(out);
    }
    writeln!(out, "Found {} notes.", notes.len()).unwrap();
    for note in notes {
        writeln!(
            out,
            "{} [{}] {}:{}",
            note.note_id,
            note.status.as_str(),
            note.target.kind.as_str(),
            note.target.id
        )
        .unwrap();
        writeln!(out, "  {}", note.claim).unwrap();
    }
    Ok(out)
}

fn render_note_mutation(
    action: &str,
    note: &AgentNote,
    json_output: bool,
) -> anyhow::Result<String> {
    if json_output {
        return Ok(format!("{}\n", serde_json::to_string_pretty(note)?));
    }
    Ok(format!(
        "Note {}: {} [{}]\n",
        action,
        note.note_id,
        note.status.as_str()
    ))
}

fn render_simple_status(action: &str, note_id: &str, json_output: bool) -> anyhow::Result<()> {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                action: true,
                "note_id": note_id,
                "source_store": "overlay",
                "advisory": true
            }))?
        );
    } else {
        println!("Note {note_id} {action}.");
    }
    Ok(())
}
