use crate::surface::card::accounting::estimate_tokens_bytes;

use super::types::{OmittedItem, ResumeContextPacket};

pub(super) fn apply_budget(packet: &mut ResumeContextPacket) {
    refresh_state(packet);
    if packet.context_state.token_estimate <= packet.context_state.token_cap {
        return;
    }
    packet.context_state.truncation_applied = true;
    if packet.context_state.metrics_hint.take().is_some() {
        packet.omitted.push(OmittedItem {
            section: "metrics_hint".to_string(),
            reason: "budget_tokens_exceeded".to_string(),
            omitted_count: 1,
        });
        refresh_state(packet);
    }
    if packet.context_state.token_estimate <= packet.context_state.token_cap {
        return;
    }
    omit_activity(packet);
    if packet.context_state.token_estimate <= packet.context_state.token_cap {
        return;
    }
    omit_notes(packet);
    if packet.context_state.token_estimate <= packet.context_state.token_cap {
        return;
    }
    omit_next_actions(packet);
    if packet.context_state.token_estimate <= packet.context_state.token_cap {
        return;
    }
    truncate_changed_files(packet, 5);
}

pub(super) fn estimate_packet_tokens(packet: &ResumeContextPacket) -> usize {
    let value = serde_json::to_vec(packet).unwrap_or_else(|_| b"{}".to_vec());
    estimate_tokens_bytes(value.len())
}

fn omit_activity(packet: &mut ResumeContextPacket) {
    let count = packet.sections.recent_activity.activity.len();
    if count == 0 {
        return;
    }
    packet.sections.recent_activity.activity.clear();
    packet.sections.recent_activity.count = 0;
    packet.omitted.push(OmittedItem {
        section: "recent_activity".to_string(),
        reason: "budget_tokens_exceeded".to_string(),
        omitted_count: count,
    });
    refresh_state(packet);
}

fn omit_notes(packet: &mut ResumeContextPacket) {
    let count = packet.sections.saved_notes.notes.len();
    if count == 0 {
        return;
    }
    packet.sections.saved_notes.notes.clear();
    packet.sections.saved_notes.count = 0;
    packet.omitted.push(OmittedItem {
        section: "saved_notes".to_string(),
        reason: "budget_tokens_exceeded".to_string(),
        omitted_count: count,
    });
    refresh_state(packet);
}

fn omit_next_actions(packet: &mut ResumeContextPacket) {
    let count = packet.sections.next_actions.items.len();
    if count == 0 {
        return;
    }
    packet.sections.next_actions.items.clear();
    packet.sections.next_actions.count = 0;
    packet.omitted.push(OmittedItem {
        section: "next_actions".to_string(),
        reason: "budget_tokens_exceeded".to_string(),
        omitted_count: count,
    });
    refresh_state(packet);
}

fn truncate_changed_files(packet: &mut ResumeContextPacket, keep: usize) {
    let count = packet.sections.changed_files.files.len();
    if count <= keep {
        refresh_state(packet);
        return;
    }
    packet.sections.changed_files.files.truncate(keep);
    packet.omitted.push(OmittedItem {
        section: "changed_files".to_string(),
        reason: "budget_tokens_exceeded".to_string(),
        omitted_count: count - keep,
    });
    refresh_state(packet);
}

fn refresh_state(packet: &mut ResumeContextPacket) {
    packet.context_state.omitted_count = packet.omitted.len();
    packet.context_state.token_estimate = estimate_packet_tokens(packet);
}
