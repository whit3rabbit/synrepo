//! Explicit repo-scoped resume packet.

mod budget;
mod notes;
mod render;
mod types;

use std::path::Path;

use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::{
    config::Config,
    pipeline::{
        context_metrics,
        recent_activity::{read_recent_activity, RecentActivityQuery},
    },
    surface::{
        changed::git_changed_files,
        handoffs::{collect_handoffs, HandoffsRequest},
    },
};

use budget::{apply_budget, estimate_packet_tokens};
use notes::read_saved_notes;
use render::detail_pointers;
pub use render::{to_json, to_markdown};
pub use types::{
    AgentNoteSummary, ChangedFilesSection, DetailPointer, MetricsHint, NextActionsSection,
    OmittedItem, RecentActivitySection, ResumeContextPacket, ResumeContextRequest,
    ResumeContextSections, ResumeContextState, SavedNotesSection, ValidationSection,
    DEFAULT_RESUME_CONTEXT_LIMIT, DEFAULT_RESUME_CONTEXT_SINCE_DAYS,
    DEFAULT_RESUME_CONTEXT_TOKEN_CAP, MAX_RESUME_CONTEXT_LIMIT, MAX_RESUME_CONTEXT_SINCE_DAYS,
};

const SCHEMA_VERSION: u32 = 1;

/// Build a bounded repo-scoped resume packet from existing synrepo state.
pub fn build_resume_context(
    repo_root: &Path,
    config: &Config,
    request: ResumeContextRequest,
) -> crate::Result<ResumeContextPacket> {
    let request = normalize_request(request);
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let generated_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|err| crate::Error::Other(anyhow::anyhow!(err)))?;
    let changed_files = git_changed_files(repo_root).unwrap_or_default();
    let handoffs = collect_handoffs(
        repo_root,
        config,
        &HandoffsRequest {
            limit: request.limit,
            since_days: request.since_days,
        },
    )
    .unwrap_or_default();
    let since = OffsetDateTime::now_utc()
        .saturating_sub(Duration::days(i64::from(request.since_days)))
        .format(&Rfc3339)
        .ok();
    let activity = read_recent_activity(
        &synrepo_dir,
        repo_root,
        config,
        RecentActivityQuery {
            kinds: None,
            limit: request.limit,
            since,
        },
    )
    .unwrap_or_default();
    let saved_notes = read_saved_notes(&synrepo_dir, request.include_notes, request.limit);
    let metrics = context_metrics::load_optional(&synrepo_dir)
        .ok()
        .flatten()
        .map(|metrics| MetricsHint {
            cards_served_total: metrics.cards_served_total,
            estimated_tokens_saved_total: metrics.estimated_tokens_saved_total,
            compact_outputs_total: metrics.compact_outputs_total,
            resume_context_responses_total: metrics.resume_context_responses_total,
        });

    let mut packet = ResumeContextPacket {
        schema_version: SCHEMA_VERSION,
        packet_type: "repo_resume_context".to_string(),
        repo_root: repo_root.display().to_string(),
        generated_at,
        context_state: ResumeContextState {
            token_estimate: 0,
            token_cap: request.budget_tokens,
            truncation_applied: false,
            omitted_count: 0,
            source_stores: vec![
                "git".to_string(),
                "operations".to_string(),
                "overlay".to_string(),
                "context_metrics".to_string(),
            ],
            metrics_hint: metrics,
        },
        sections: ResumeContextSections {
            changed_files: ChangedFilesSection {
                source_store: "git".to_string(),
                count: changed_files.len(),
                files: changed_files,
            },
            next_actions: NextActionsSection {
                source_store: "repair+overlay+git".to_string(),
                count: handoffs.len(),
                items: handoffs,
            },
            recent_activity: RecentActivitySection {
                source_store: "operations".to_string(),
                count: activity.len(),
                activity,
            },
            saved_notes,
            validation: ValidationSection {
                recommended_commands: vec![
                    "synrepo status --recent".to_string(),
                    "git status --short".to_string(),
                    "synrepo check".to_string(),
                    "synrepo tests <changed-path>".to_string(),
                ],
            },
        },
        detail_pointers: detail_pointers(request.limit, request.since_days, request.include_notes),
        omitted: Vec::new(),
    };

    apply_budget(&mut packet);
    let final_tokens = estimate_packet_tokens(&packet);
    context_metrics::record_resume_context_best_effort(&synrepo_dir, final_tokens);
    Ok(packet)
}

fn normalize_request(mut request: ResumeContextRequest) -> ResumeContextRequest {
    request.limit = request.limit.clamp(1, MAX_RESUME_CONTEXT_LIMIT);
    request.since_days = request.since_days.clamp(1, MAX_RESUME_CONTEXT_SINCE_DAYS);
    request.budget_tokens = request.budget_tokens.clamp(1, 12_000);
    request
}

#[cfg(test)]
mod tests;
