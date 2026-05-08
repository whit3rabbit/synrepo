//! Text formatter for `synrepo status`.

use std::fmt::Write;

use synrepo::{
    pipeline::diagnostics::{EmbeddingHealth, ReconcileHealth, WriterStatus},
    surface::readiness::ReadinessMatrix,
    surface::status_snapshot::StatusSnapshot,
};

use super::helpers;

pub(super) fn write_status_text(
    out: &mut String,
    snapshot: &StatusSnapshot,
    readiness: Option<&ReadinessMatrix>,
    full: bool,
) {
    if !snapshot.initialized {
        writeln!(out, "synrepo status: not initialized").unwrap();
        writeln!(
            out,
            "  Run `synrepo init` to create .synrepo/ and populate the graph."
        )
        .unwrap();
        return;
    }
    let config = snapshot
        .config
        .as_ref()
        .expect("initialized implies config loaded");
    let diag = snapshot
        .diagnostics
        .as_ref()
        .expect("initialized implies diagnostics present");

    writeln!(out, "synrepo status").unwrap();
    writeln!(out, "  mode:         {}", config.mode).unwrap();

    match &snapshot.graph_stats {
        Some(stats) => writeln!(
            out,
            "  graph:        {} files  {} symbols  {} concepts",
            stats.file_nodes, stats.symbol_nodes, stats.concept_nodes
        )
        .unwrap(),
        None => writeln!(out, "  graph:        not materialized — run `synrepo init`").unwrap(),
    }

    match &diag.reconcile_health {
        ReconcileHealth::Current => writeln!(out, "  reconcile:    current").unwrap(),
        ReconcileHealth::Stale(synrepo::pipeline::diagnostics::ReconcileStaleness::Outcome(o)) => {
            writeln!(out, "  reconcile:    stale (last outcome: {o})").unwrap()
        }
        ReconcileHealth::Stale(synrepo::pipeline::diagnostics::ReconcileStaleness::Age {
            ..
        }) => writeln!(out, "  reconcile:    stale (over 1 hour old)").unwrap(),
        ReconcileHealth::WatchStalled { last_reconcile_at } => writeln!(
            out,
            "  reconcile:    watch_stalled (watch up but last reconcile {last_reconcile_at} > 1h)"
        )
        .unwrap(),
        ReconcileHealth::Unknown => writeln!(out, "  reconcile:    unknown (never run)").unwrap(),
        ReconcileHealth::Corrupt(e) => writeln!(out, "  reconcile:    corrupt ({e})").unwrap(),
    }

    if let Some(state) = &diag.last_reconcile {
        let detail = match (state.files_discovered, state.symbols_extracted) {
            (Some(files), Some(symbols)) => {
                format!(
                    "completed — {files} files, {symbols} symbols ({} events)",
                    state.triggering_events
                )
            }
            _ => format!(
                "{} ({} events)",
                state.last_outcome, state.triggering_events
            ),
        };
        writeln!(
            out,
            "  last run:     {} — {detail}",
            state.last_reconcile_at
        )
        .unwrap();
        if let Some(error) = &state.last_error {
            writeln!(out, "  error:        {error}").unwrap();
        }
    }

    writeln!(
        out,
        "  watch:        {}",
        helpers::render_watch_summary(&diag.watch_status)
    )
    .unwrap();

    match &diag.writer_status {
        WriterStatus::Free => writeln!(out, "  writer lock:  free").unwrap(),
        WriterStatus::HeldBySelf => writeln!(out, "  writer lock:  held by this process").unwrap(),
        WriterStatus::HeldByOther { pid } => {
            writeln!(out, "  writer lock:  held by pid {pid}").unwrap()
        }
        WriterStatus::Corrupt(e) => writeln!(out, "  writer lock:  corrupt ({e})").unwrap(),
    }

    for line in &diag.store_guidance {
        writeln!(out, "  store:        {line}").unwrap();
    }

    writeln!(
        out,
        "  commentary:   {}",
        snapshot.commentary_coverage.display
    )
    .unwrap();
    if let Some(counts) = snapshot.agent_note_counts {
        writeln!(
            out,
            "  agent notes:  {} active  {} stale  {} unverified  {} superseded  {} forgotten  {} invalid",
            counts.active,
            counts.stale,
            counts.unverified,
            counts.superseded,
            counts.forgotten,
            counts.invalid
        )
        .unwrap();
    }
    writeln!(out, "  context export: {}", snapshot.export_status.display).unwrap();
    writeln!(out, "  overlay cost: {}", snapshot.overlay_cost_summary).unwrap();
    if full {
        writeln!(
            out,
            "  snapshot:     epoch {}  age {}  size {} ({} files / {} symbols / {} edges)",
            snapshot.graph_snapshot.epoch,
            render_snapshot_age(snapshot.graph_snapshot.age_ms),
            render_snapshot_size(snapshot.graph_snapshot.size_bytes),
            snapshot.graph_snapshot.file_count,
            snapshot.graph_snapshot.symbol_count,
            snapshot.graph_snapshot.edge_count,
        )
        .unwrap();
    }
    match &diag.embedding_health {
        EmbeddingHealth::Disabled => {
            writeln!(
                out,
                "  embedding:    off (optional; lexical routing/search still available)"
            )
            .unwrap();
        }
        EmbeddingHealth::Available {
            provider,
            provider_source,
            model,
            dim,
            chunks,
        } => {
            writeln!(
                out,
                "  embedding:    available ({}/{model}{}, {dim}d, {chunks} chunks)",
                provider,
                provider_source_suffix(*provider_source)
            )
            .unwrap();
        }
        EmbeddingHealth::Degraded {
            provider,
            provider_source,
            reason,
        } => {
            writeln!(
                out,
                "  embedding:    degraded ({}/{model}{}: {reason})",
                provider,
                provider_source_suffix(*provider_source),
                model = config.semantic_model.as_str()
            )
            .unwrap();
        }
    }
    if let Some(ts) = snapshot.last_compaction {
        writeln!(out, "  last compact:  {}", ts).unwrap();
    } else {
        writeln!(out, "  last compact:  never").unwrap();
    }
    writeln!(
        out,
        "  repair audit: {}",
        helpers::render_repair_audit(&snapshot.repair_audit)
    )
    .unwrap();

    // Explain provider status
    if let Some(explain) = &snapshot.explain_provider {
        writeln!(out, "  explain:    {}", render_explain_line(explain)).unwrap();
    } else {
        writeln!(out, "  explain:    not initialized").unwrap();
    }
    if let Some(totals) = &snapshot.explain_totals {
        let total_calls = totals.calls + totals.failures + totals.budget_blocked;
        if total_calls > 0 {
            let openrouter_live = super::openrouter_live_pricing_used(totals);
            let est = if totals.any_estimated { " (est.)" } else { "" };
            let cost = if totals.any_unpriced {
                format!("${:.4} + unpriced", totals.usd_cost)
            } else {
                format!("${:.4}", totals.usd_cost)
            };
            writeln!(
                out,
                "    usage:      {} call(s), {} in / {} out tokens{}, {} (pricing as of {})",
                total_calls,
                totals.input_tokens,
                totals.output_tokens,
                est,
                cost,
                synrepo::pipeline::explain::pricing::pricing_basis_label(openrouter_live)
            )
            .unwrap();
            if totals.failures > 0 || totals.budget_blocked > 0 {
                writeln!(
                    out,
                    "    skipped:    {} failed, {} budget-blocked",
                    totals.failures, totals.budget_blocked
                )
                .unwrap();
            }
        }
    }

    if let Some(metrics) = &snapshot.context_metrics {
        if metrics.cards_served_total > 0 {
            writeln!(
                out,
                "  context:    {} card(s), {:.1} avg tokens/card, {} est. tokens avoided",
                metrics.cards_served_total,
                metrics.card_tokens_avg(),
                metrics.estimated_tokens_saved_total
            )
            .unwrap();
        }
        if metrics.mcp_requests_total > 0 {
            let tool_errors: u64 = metrics.mcp_tool_errors_total.values().sum();
            writeln!(
                out,
                "  mcp:        {} request(s), {} resource read(s), {} tool error(s)",
                metrics.mcp_requests_total, metrics.mcp_resource_reads_total, tool_errors
            )
            .unwrap();
            let saved_context_writes: u64 = metrics.saved_context_writes_total.values().sum();
            if saved_context_writes > 0 {
                writeln!(
                    out,
                    "  saved ctx:  {} explicit advisory note mutation(s)",
                    saved_context_writes
                )
                .unwrap();
            }
        }
        if metrics.compact_outputs_total > 0 {
            writeln!(
                out,
                "  compact:    {} output(s), {} est. tokens avoided, {} omitted item(s)",
                metrics.compact_outputs_total,
                metrics.compact_estimated_tokens_saved_total,
                metrics.compact_omitted_items_total
            )
            .unwrap();
        }
        if metrics.route_classifications_total > 0
            || metrics.context_fast_path_signals_total > 0
            || metrics.deterministic_edit_candidates_total > 0
        {
            writeln!(
                out,
                "  fast path:  {} route(s), {} context signal(s), {} edit candidate(s), {} est. LLM call(s) avoided",
                metrics.route_classifications_total,
                metrics.context_fast_path_signals_total,
                metrics.deterministic_edit_candidates_total,
                metrics.estimated_llm_calls_avoided_total
            )
            .unwrap();
        }
        if metrics.anchored_edit_accepted_total > 0 || metrics.anchored_edit_rejected_total > 0 {
            writeln!(
                out,
                "  anchors:    {} accepted edit(s), {} rejected edit(s)",
                metrics.anchored_edit_accepted_total, metrics.anchored_edit_rejected_total
            )
            .unwrap();
        }
    }

    writeln!(
        out,
        "  next step:    {}",
        helpers::next_step(diag, snapshot.graph_stats.is_none())
    )
    .unwrap();

    if let Some(matrix) = readiness {
        writeln!(out).unwrap();
        writeln!(out, "capability readiness:").unwrap();
        for row in &matrix.rows {
            let action = match &row.next_action {
                Some(a) => format!(" — {a}"),
                None => String::new(),
            };
            writeln!(
                out,
                "  {:<18} {:<12} {}{}",
                row.label(),
                row.state.as_str(),
                row.detail,
                action
            )
            .unwrap();
        }
    }

    if let Some(entries) = &snapshot.recent_activity {
        writeln!(out).unwrap();
        writeln!(out, "recent activity:").unwrap();
        if entries.is_empty() {
            writeln!(out, "  (none)").unwrap();
        }
        for entry in entries {
            if entry.timestamp.is_empty() {
                writeln!(out, "  [{}] {}", entry.kind, entry.payload).unwrap();
            } else {
                writeln!(
                    out,
                    "  {} [{}] {}",
                    entry.timestamp, entry.kind, entry.payload
                )
                .unwrap();
            }
        }
    }
}

fn provider_source_suffix(source: synrepo::config::SemanticProviderSource) -> &'static str {
    match source {
        synrepo::config::SemanticProviderSource::Explicit => "",
        synrepo::config::SemanticProviderSource::Defaulted => " (defaulted)",
    }
}

fn render_snapshot_age(age_ms: u64) -> String {
    if age_ms < 1_000 {
        format!("{age_ms}ms")
    } else {
        format!("{:.1}s", age_ms as f64 / 1_000.0)
    }
}

fn render_snapshot_size(size_bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if size_bytes >= 1024 * 1024 {
        format!("{:.1} MiB", size_bytes as f64 / MIB)
    } else if size_bytes >= 1024 {
        format!("{:.1} KiB", size_bytes as f64 / KIB)
    } else {
        format!("{size_bytes} B")
    }
}

fn render_explain_line(display: &synrepo::surface::status_snapshot::ExplainDisplay) -> String {
    use std::fmt::Write as _;
    use synrepo::pipeline::explain::ExplainStatus;

    let provider_and_model = match &display.model {
        Some(m) => format!("{} ({})", display.provider, m),
        None => display.provider.clone(),
    };
    match &display.status {
        ExplainStatus::Enabled => {
            let mut line = provider_and_model;
            if let Some(endpoint) = &display.local_endpoint {
                let source = display.endpoint_source.display_label();
                write!(line, " @ {endpoint} [source: {source}]").unwrap();
            }
            line
        }
        ExplainStatus::DisabledKeyDetected { env_var } => {
            format!(
                "disabled ({env_var} detected; run 'synrepo setup <tool> --explain' \
                 to enable, or set [explain] enabled = true in .synrepo/config.toml \
                 and store reusable keys or local endpoints in ~/.synrepo/config.toml)"
            )
        }
        ExplainStatus::Disabled => "disabled".to_string(),
    }
}
