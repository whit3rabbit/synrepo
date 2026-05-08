use super::types::{DetailPointer, ResumeContextPacket};

/// Render a resume-context packet as pretty JSON.
pub fn to_json(packet: &ResumeContextPacket) -> String {
    serde_json::to_string_pretty(packet).unwrap_or_else(|_| "{}".to_string())
}

/// Render a resume-context packet as operator-readable Markdown.
pub fn to_markdown(packet: &ResumeContextPacket) -> String {
    let mut out = String::new();
    out.push_str("# Repo Resume Context\n\n");
    out.push_str(&format!(
        "Generated: {}\n\nToken estimate: {} / {}\n\n",
        packet.generated_at, packet.context_state.token_estimate, packet.context_state.token_cap
    ));
    push_list(
        &mut out,
        "Changed Files",
        &packet.sections.changed_files.files,
    );
    out.push_str("## Next Actions\n\n");
    if packet.sections.next_actions.items.is_empty() {
        out.push_str("None.\n\n");
    } else {
        for item in &packet.sections.next_actions.items {
            out.push_str(&format!(
                "- [{}] {} ({})\n",
                item.priority.as_str(),
                item.recommendation,
                item.source
            ));
        }
        out.push('\n');
    }
    out.push_str("## Saved Notes\n\n");
    match packet.sections.saved_notes.overlay_state.as_str() {
        "available" if packet.sections.saved_notes.notes.is_empty() => out.push_str("None.\n\n"),
        "available" => {
            for note in &packet.sections.saved_notes.notes {
                out.push_str(&format!(
                    "- {} [{}] {}: {}\n",
                    note.note_id, note.status, note.target, note.claim_preview
                ));
            }
            out.push('\n');
        }
        other => out.push_str(&format!("{other}.\n\n")),
    }
    out.push_str("## Recent Activity\n\n");
    if packet.sections.recent_activity.activity.is_empty() {
        out.push_str("None.\n\n");
    } else {
        for entry in &packet.sections.recent_activity.activity {
            out.push_str(&format!("- {} {}\n", entry.kind, entry.timestamp));
        }
        out.push('\n');
    }
    push_list(
        &mut out,
        "Validation",
        &packet.sections.validation.recommended_commands,
    );
    if !packet.omitted.is_empty() {
        out.push_str("## Omitted\n\n");
        for item in &packet.omitted {
            out.push_str(&format!(
                "- {}: {} ({} items)\n",
                item.section, item.reason, item.omitted_count
            ));
        }
        out.push('\n');
    }
    out.push_str("## Detail Pointers\n\n");
    for pointer in &packet.detail_pointers {
        out.push_str(&format!(
            "- {}: `{}` or `{}`\n",
            pointer.label, pointer.mcp, pointer.cli
        ));
    }
    out
}

pub(super) fn detail_pointers(
    limit: usize,
    since_days: u32,
    include_notes: bool,
) -> Vec<DetailPointer> {
    let mut pointers = vec![
        DetailPointer {
            label: "changed files".to_string(),
            mcp: "synrepo_changed".to_string(),
            cli: "git status --short".to_string(),
        },
        DetailPointer {
            label: "next actions".to_string(),
            mcp: format!("synrepo_next_actions(limit: {limit}, since_days: {since_days})"),
            cli: format!("synrepo handoffs --limit {limit} --since {since_days}"),
        },
        DetailPointer {
            label: "recent activity".to_string(),
            mcp: format!("synrepo_recent_activity(limit: {limit})"),
            cli: "synrepo status --recent".to_string(),
        },
    ];
    if include_notes {
        pointers.push(DetailPointer {
            label: "saved notes".to_string(),
            mcp: format!("synrepo_notes(limit: {limit})"),
            cli: format!("synrepo notes list --limit {limit}"),
        });
    }
    pointers
}

fn push_list(out: &mut String, title: &str, items: &[String]) {
    out.push_str(&format!("## {title}\n\n"));
    if items.is_empty() {
        out.push_str("None.\n\n");
    } else {
        for item in items {
            out.push_str(&format!("- `{item}`\n"));
        }
        out.push('\n');
    }
}
