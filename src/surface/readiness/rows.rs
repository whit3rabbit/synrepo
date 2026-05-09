use crate::{
    bootstrap::runtime_probe::{Missing, ProbeReport, RuntimeClassification},
    config::Config,
    pipeline::{
        diagnostics::{EmbeddingHealth, ReconcileHealth, ReconcileStaleness},
        git::{GitDegradedReason, GitIntelligenceContext, GitIntelligenceReadiness},
        watch::WatchServiceStatus,
    },
    surface::status_snapshot::{OverlayState, StatusSnapshot},
};

use super::{Capability, ReadinessRow, ReadinessState};

pub(super) fn parser_row(snapshot: &StatusSnapshot) -> ReadinessRow {
    // Parser coverage follows reconcile history: a completed reconcile with a
    // non-zero file count implies the parser ran. We do not track per-file
    // parser failures at this layer; downstream surfaces still carry Epistemic
    // labels that distinguish parser-observed truth from machine inference.
    let Some(diag) = snapshot.diagnostics.as_ref() else {
        return ReadinessRow {
            capability: Capability::Parser,
            state: ReadinessState::Unavailable,
            detail: "not initialized".to_string(),
            next_action: Some("run `synrepo init`".to_string()),
        };
    };

    match &diag.reconcile_health {
        ReconcileHealth::Current | ReconcileHealth::Stale(ReconcileStaleness::Age { .. }) => {
            let files = diag
                .last_reconcile
                .as_ref()
                .and_then(|s| s.files_discovered);
            match files {
                Some(n) => ReadinessRow {
                    capability: Capability::Parser,
                    state: ReadinessState::Supported,
                    detail: format!("{n} files discovered"),
                    next_action: None,
                },
                None => ReadinessRow {
                    capability: Capability::Parser,
                    state: ReadinessState::Degraded,
                    detail: "reconcile completed but counts unavailable".to_string(),
                    next_action: Some("run `synrepo reconcile`".to_string()),
                },
            }
        }
        ReconcileHealth::Stale(ReconcileStaleness::Outcome(outcome)) => ReadinessRow {
            capability: Capability::Parser,
            state: ReadinessState::Degraded,
            detail: format!("last reconcile {outcome}"),
            next_action: Some("run `synrepo check` then `synrepo sync`".to_string()),
        },
        ReconcileHealth::WatchStalled { last_reconcile_at } => ReadinessRow {
            capability: Capability::Parser,
            state: ReadinessState::Degraded,
            detail: format!("watch up but last reconcile {last_reconcile_at} > 1h"),
            next_action: Some(
                "run `synrepo watch stop` then `synrepo watch` to restart the watch loop"
                    .to_string(),
            ),
        },
        ReconcileHealth::Unknown => ReadinessRow {
            capability: Capability::Parser,
            state: ReadinessState::Stale,
            detail: "no reconcile has run yet".to_string(),
            next_action: Some("run `synrepo reconcile`".to_string()),
        },
        ReconcileHealth::Corrupt(e) => ReadinessRow {
            capability: Capability::Parser,
            state: ReadinessState::Blocked,
            detail: format!("reconcile state corrupt ({e})"),
            next_action: Some("run `synrepo watch stop` and re-reconcile".to_string()),
        },
    }
}

pub(super) fn git_row(repo_root: &std::path::Path, config: &Config) -> ReadinessRow {
    let ctx = GitIntelligenceContext::inspect(repo_root, config);
    match ctx.readiness() {
        GitIntelligenceReadiness::Ready => ReadinessRow {
            capability: Capability::GitIntelligence,
            state: ReadinessState::Supported,
            detail: "repository attached, full history".to_string(),
            next_action: None,
        },
        GitIntelligenceReadiness::Degraded { reasons } => {
            let repo_unavailable = reasons
                .iter()
                .any(|r| matches!(r, GitDegradedReason::RepositoryUnavailable));
            let detail = summarize_git_reasons(&reasons);
            if repo_unavailable {
                ReadinessRow {
                    capability: Capability::GitIntelligence,
                    state: ReadinessState::Unavailable,
                    detail,
                    next_action: Some(
                        "initialize git with `git init` to enable history-derived facts"
                            .to_string(),
                    ),
                }
            } else {
                ReadinessRow {
                    capability: Capability::GitIntelligence,
                    state: ReadinessState::Degraded,
                    detail,
                    next_action: Some(
                        "restore branch attachment or unshallow with `git fetch --unshallow`"
                            .to_string(),
                    ),
                }
            }
        }
    }
}

fn summarize_git_reasons(reasons: &[GitDegradedReason]) -> String {
    let mut parts: Vec<&'static str> = reasons
        .iter()
        .map(|r| match r {
            GitDegradedReason::ShallowHistory => "shallow history",
            GitDegradedReason::DetachedHead => "detached HEAD",
            GitDegradedReason::UnbornHead => "unborn HEAD (no commits)",
            GitDegradedReason::RepositoryUnavailable => "no git repository",
        })
        .collect();
    parts.dedup();
    parts.join(", ")
}

pub(super) fn embeddings_row(snapshot: &StatusSnapshot, config: &Config) -> ReadinessRow {
    let Some(diag) = snapshot.diagnostics.as_ref() else {
        return ReadinessRow {
            capability: Capability::Embeddings,
            state: ReadinessState::Disabled,
            detail: "not initialized".to_string(),
            next_action: None,
        };
    };
    match &diag.embedding_health {
        EmbeddingHealth::Disabled => ReadinessRow {
            capability: Capability::Embeddings,
            state: ReadinessState::Disabled,
            detail: "optional; semantic routing uses lexical fallback".to_string(),
            next_action: if config.enable_semantic_triage {
                Some("rebuild with `--features semantic-triage`".to_string())
            } else {
                None
            },
        },
        EmbeddingHealth::Available {
            provider,
            provider_source,
            model,
            dim,
            chunks,
        } => ReadinessRow {
            capability: Capability::Embeddings,
            state: ReadinessState::Supported,
            detail: format!(
                "{provider}/{model}{} ({dim}d, {chunks} chunks)",
                provider_source_suffix(*provider_source)
            ),
            next_action: None,
        },
        EmbeddingHealth::Degraded {
            provider,
            provider_source,
            reason,
        } => ReadinessRow {
            capability: Capability::Embeddings,
            state: ReadinessState::Degraded,
            detail: format!(
                "{}/{}{}: {reason}",
                provider,
                config.semantic_model.as_str(),
                provider_source_suffix(*provider_source)
            ),
            next_action: Some("run `synrepo embeddings build` to rebuild the index".to_string()),
        },
    }
}

fn provider_source_suffix(source: crate::config::SemanticProviderSource) -> &'static str {
    match source {
        crate::config::SemanticProviderSource::Explicit => "",
        crate::config::SemanticProviderSource::Defaulted => " (defaulted)",
    }
}

pub(super) fn watch_row(snapshot: &StatusSnapshot) -> ReadinessRow {
    let Some(diag) = snapshot.diagnostics.as_ref() else {
        return ReadinessRow {
            capability: Capability::Watch,
            state: ReadinessState::Disabled,
            detail: "not initialized".to_string(),
            next_action: None,
        };
    };
    match &diag.watch_status {
        WatchServiceStatus::Running(state) => ReadinessRow {
            capability: Capability::Watch,
            state: ReadinessState::Supported,
            detail: format!("{} (pid {})", state.mode, state.pid),
            next_action: None,
        },
        WatchServiceStatus::Starting => ReadinessRow {
            capability: Capability::Watch,
            state: ReadinessState::Supported,
            detail: "starting".to_string(),
            next_action: None,
        },
        WatchServiceStatus::Inactive => ReadinessRow {
            capability: Capability::Watch,
            state: ReadinessState::Disabled,
            detail: "not running".to_string(),
            next_action: Some(
                "run `synrepo watch` or `synrepo watch --daemon` to enable".to_string(),
            ),
        },
        WatchServiceStatus::Stale(state) => {
            let detail = match state {
                Some(s) => format!("stale owner (pid {})", s.pid),
                None => "stale artifacts".to_string(),
            };
            ReadinessRow {
                capability: Capability::Watch,
                state: ReadinessState::Stale,
                detail,
                next_action: Some("run `synrepo watch stop` to clean up".to_string()),
            }
        }
        WatchServiceStatus::Corrupt(e) => ReadinessRow {
            capability: Capability::Watch,
            state: ReadinessState::Blocked,
            detail: format!("corrupt ({e})"),
            next_action: Some("run `synrepo watch stop` and inspect logs".to_string()),
        },
    }
}

pub(super) fn index_freshness_row(snapshot: &StatusSnapshot) -> ReadinessRow {
    let Some(diag) = snapshot.diagnostics.as_ref() else {
        return ReadinessRow {
            capability: Capability::IndexFreshness,
            state: ReadinessState::Unavailable,
            detail: "not initialized".to_string(),
            next_action: Some("run `synrepo init`".to_string()),
        };
    };
    match &diag.reconcile_health {
        ReconcileHealth::Current => ReadinessRow {
            capability: Capability::IndexFreshness,
            state: ReadinessState::Supported,
            detail: "reconcile current".to_string(),
            next_action: None,
        },
        ReconcileHealth::Stale(ReconcileStaleness::Age { last_reconcile_at }) => ReadinessRow {
            capability: Capability::IndexFreshness,
            state: ReadinessState::Stale,
            detail: format!("last reconcile {last_reconcile_at}"),
            next_action: Some("run `synrepo reconcile`".to_string()),
        },
        ReconcileHealth::Stale(ReconcileStaleness::Outcome(outcome)) => ReadinessRow {
            capability: Capability::IndexFreshness,
            state: ReadinessState::Stale,
            detail: format!("last outcome {outcome}"),
            next_action: Some("run `synrepo reconcile`".to_string()),
        },
        ReconcileHealth::WatchStalled { last_reconcile_at } => ReadinessRow {
            capability: Capability::IndexFreshness,
            state: ReadinessState::Stale,
            detail: format!("watch up but last reconcile {last_reconcile_at} > 1h"),
            next_action: Some(
                "run `synrepo watch stop` then `synrepo watch` to restart the watch loop"
                    .to_string(),
            ),
        },
        ReconcileHealth::Unknown => ReadinessRow {
            capability: Capability::IndexFreshness,
            state: ReadinessState::Stale,
            detail: "no reconcile recorded".to_string(),
            next_action: Some("run `synrepo reconcile`".to_string()),
        },
        ReconcileHealth::Corrupt(e) => ReadinessRow {
            capability: Capability::IndexFreshness,
            state: ReadinessState::Blocked,
            detail: format!("corrupt ({e})"),
            next_action: Some("run `synrepo watch stop` and re-reconcile".to_string()),
        },
    }
}

pub(super) fn overlay_row(snapshot: &StatusSnapshot) -> ReadinessRow {
    match snapshot.overlay_state {
        OverlayState::Ready => ReadinessRow {
            capability: Capability::Overlay,
            state: ReadinessState::Supported,
            detail: snapshot.commentary_coverage.display.clone(),
            next_action: None,
        },
        OverlayState::ReadyEmpty => ReadinessRow {
            capability: Capability::Overlay,
            state: ReadinessState::Supported,
            detail: "ready_empty; no overlay entries yet".to_string(),
            next_action: None,
        },
        OverlayState::Missing => ReadinessRow {
            capability: Capability::Overlay,
            state: ReadinessState::Unavailable,
            detail: "overlay store missing".to_string(),
            next_action: Some("run `synrepo init`".to_string()),
        },
        OverlayState::Error => ReadinessRow {
            capability: Capability::Overlay,
            state: ReadinessState::Blocked,
            detail: snapshot.commentary_coverage.display.clone(),
            next_action: Some("run `synrepo check` to evaluate repair actions".to_string()),
        },
    }
}

pub(super) fn compatibility_row(probe: &ProbeReport, snapshot: &StatusSnapshot) -> ReadinessRow {
    if let RuntimeClassification::Partial { missing } = &probe.classification {
        for m in missing {
            match m {
                Missing::CompatBlocked { guidance } => {
                    let detail = guidance
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "compatibility blocked".to_string());
                    return ReadinessRow {
                        capability: Capability::Compatibility,
                        state: ReadinessState::Blocked,
                        detail,
                        next_action: Some("run `synrepo upgrade`".to_string()),
                    };
                }
                Missing::CompatEvaluationFailed { detail } => {
                    return ReadinessRow {
                        capability: Capability::Compatibility,
                        state: ReadinessState::Blocked,
                        detail: format!("evaluation failed: {detail}"),
                        next_action: Some("run `synrepo upgrade` and retry".to_string()),
                    };
                }
                _ => {}
            }
        }
    }
    if let Some(diag) = snapshot.diagnostics.as_ref() {
        if !diag.store_guidance.is_empty() {
            let detail = diag.store_guidance.first().cloned().unwrap_or_default();
            return ReadinessRow {
                capability: Capability::Compatibility,
                state: ReadinessState::Stale,
                detail,
                next_action: Some("run `synrepo upgrade --apply`".to_string()),
            };
        }
    }
    ReadinessRow {
        capability: Capability::Compatibility,
        state: ReadinessState::Supported,
        detail: "stores compatible".to_string(),
        next_action: None,
    }
}
