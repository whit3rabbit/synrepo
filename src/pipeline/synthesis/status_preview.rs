use std::path::{Path, PathBuf};

use crate::{
    config::Config,
    pipeline::{
        git::GitIntelligenceContext,
        git_intelligence::analyze_recent_history,
        repair::{load_commentary_work_plan, scan_commentary_staleness, CommentaryWorkItem},
    },
};

use super::{describe_active_provider, ActiveProvider, SynthesisStatus};

/// Number of sample targets kept per queued-work group.
pub const SAMPLE_LIMIT_PER_GROUP: usize = 3;

/// One queued-work group within a synthesis preview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SynthesisPreviewGroup {
    /// Human-readable group label.
    pub label: &'static str,
    /// Total number of queued targets in the group.
    pub total_count: usize,
    /// Sample targets from the group, already formatted for display.
    pub items: Vec<String>,
    /// Number of omitted targets beyond the sampled `items`.
    pub remaining_count: usize,
}

/// Display-ready summary of what `synrepo synthesize` would do for a scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SynthesisPreview {
    /// Human-readable scope label.
    pub scope_label: String,
    /// Display label for the active provider and model.
    pub provider_label: String,
    /// Whether provider API calls would happen for this run.
    pub api_status_line: String,
    /// Overlay freshness summary for the whole repo.
    pub overlay_freshness_line: String,
    /// Targets with stale commentary that would be refreshed.
    pub refresh: SynthesisPreviewGroup,
    /// Files lacking commentary that would be seeded.
    pub file_seeds: SynthesisPreviewGroup,
    /// Symbols lacking commentary that would be seeded.
    pub symbol_seeds: SynthesisPreviewGroup,
    /// Files in scope that the planner checked.
    pub scoped_file_count: usize,
    /// Symbols in scope that the planner checked.
    pub scoped_symbol_count: usize,
    /// Upper bound on total targets considered for this scope.
    pub max_target_count: usize,
    /// Number of samples retained per group.
    pub sample_limit_per_group: usize,
    /// Summary line describing whether a run would do anything.
    pub summary_line: String,
}

/// Build a queued-work preview for the requested synthesize scope.
pub fn build_synthesis_preview(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
) -> anyhow::Result<SynthesisPreview> {
    let config = Config::load(repo_root).map_err(|e| {
        anyhow::anyhow!("synthesize status: not initialized — run `synrepo init` first ({e})")
    })?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let scope = compute_scope(repo_root, &config, paths, changed)?;
    let plan = load_commentary_work_plan(&synrepo_dir, scope.as_deref())
        .map_err(|e| anyhow::anyhow!("synthesize status: cannot plan commentary work ({e})"))?;
    let provider = describe_active_provider(&config);

    Ok(SynthesisPreview {
        scope_label: scope_label(changed, &scope),
        provider_label: provider_label(&provider),
        api_status_line: api_status_line(&provider.status, provider.provider),
        overlay_freshness_line: overlay_freshness_line(&synrepo_dir),
        refresh: build_group("stale commentary to refresh", &plan.refresh),
        file_seeds: build_group("files missing commentary", &plan.file_seeds),
        symbol_seeds: build_group("symbols missing commentary", &plan.symbol_seed_candidates),
        scoped_file_count: plan.scoped_file_count(),
        scoped_symbol_count: plan.scoped_symbol_count(),
        max_target_count: plan.max_target_count(),
        sample_limit_per_group: SAMPLE_LIMIT_PER_GROUP,
        summary_line: summary_line(
            plan.scoped_file_count(),
            plan.scoped_symbol_count(),
            plan.max_target_count(),
            plan.is_empty(),
        ),
    })
}

fn build_group(label: &'static str, items: &[CommentaryWorkItem]) -> SynthesisPreviewGroup {
    let sample_items: Vec<String> = items
        .iter()
        .take(SAMPLE_LIMIT_PER_GROUP)
        .map(render_target)
        .collect();
    SynthesisPreviewGroup {
        label,
        total_count: items.len(),
        remaining_count: items.len().saturating_sub(sample_items.len()),
        items: sample_items,
    }
}

fn summary_line(
    scoped_file_count: usize,
    scoped_symbol_count: usize,
    max_target_count: usize,
    plan_is_empty: bool,
) -> String {
    if plan_is_empty {
        format!(
            "checked {scoped_file_count} file(s) and {scoped_symbol_count} symbol(s) in scope. nothing currently needs synthesis for this scope."
        )
    } else {
        format!(
            "checked {scoped_file_count} file(s) and {scoped_symbol_count} symbol(s) in scope. {max_target_count} target(s) would be reconsidered if you run `synrepo synthesize` now."
        )
    }
}

fn overlay_freshness_line(synrepo_dir: &Path) -> String {
    match scan_commentary_staleness(synrepo_dir) {
        Ok(scan) => {
            let fresh = scan.total.saturating_sub(scan.stale);
            format!("{fresh} fresh, {} stale, {} total", scan.stale, scan.total)
        }
        Err(err) => format!("unavailable ({err})"),
    }
}

fn compute_scope(
    repo_root: &Path,
    config: &Config,
    paths: Vec<String>,
    changed: bool,
) -> anyhow::Result<Option<Vec<PathBuf>>> {
    if changed {
        let context = GitIntelligenceContext::inspect(repo_root, config);
        let insights = analyze_recent_history(&context, 50, 50)
            .map_err(|e| anyhow::anyhow!("synthesize status: cannot sample git history ({e})"))?;
        let hotspot_paths: Vec<PathBuf> = insights
            .hotspots
            .iter()
            .map(|h| PathBuf::from(&h.path))
            .collect();
        Ok(Some(hotspot_paths))
    } else if paths.is_empty() {
        Ok(None)
    } else {
        Ok(Some(paths.into_iter().map(PathBuf::from).collect()))
    }
}

fn scope_label(changed: bool, scope: &Option<Vec<PathBuf>>) -> String {
    if changed {
        match scope {
            Some(scope) if scope.is_empty() => {
                "files changed in the last 50 commits (none found)".to_string()
            }
            _ => "files changed in the last 50 commits".to_string(),
        }
    } else {
        match scope {
            None => "the whole repository".to_string(),
            Some(scope) if scope.is_empty() => "no matching files".to_string(),
            Some(scope) => format!(
                "selected paths: {}",
                scope
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

fn provider_label(active: &ActiveProvider) -> String {
    match &active.model {
        Some(model) => format!("{} / {model}", active.provider),
        None => active.provider.to_string(),
    }
}

fn api_status_line(status: &SynthesisStatus, provider: &str) -> String {
    match status {
        SynthesisStatus::Enabled => format!(
            "yes, [{provider}] may be called to generate advisory commentary under .synrepo/, and that may cost money depending on provider billing"
        ),
        SynthesisStatus::Disabled => {
            "no, synthesis is disabled so no provider requests will be made".to_string()
        }
        SynthesisStatus::DisabledKeyDetected { env_var } => {
            format!("no, synthesis is disabled even though ${env_var} is set")
        }
    }
}

fn render_target(item: &CommentaryWorkItem) -> String {
    match &item.qualified_name {
        Some(name) => format!("{} :: {}", item.path, name),
        None => item.path.clone(),
    }
}
