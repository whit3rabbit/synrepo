use synrepo::config::{Config, SemanticEmbeddingProvider};
use synrepo::core::ids::{NodeId, SymbolNodeId};
use synrepo::overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore};
use synrepo::pipeline::explain::accounting::{ExplainTotals, ProviderTotals};
use synrepo::pipeline::writer::{writer_lock_path, WriterOwnership};
use synrepo::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;
use time::OffsetDateTime;

use super::{seed_graph, status_output, write_explain_totals, EnvGuard};

#[test]
fn status_reports_graph_counts_after_bootstrap() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["initialized"], true);
    assert_eq!(json["graph"]["file_nodes"], 1);
    assert_eq!(json["graph"]["symbol_nodes"], 1);
    assert_eq!(json["graph"]["concept_nodes"], 1);
    assert_eq!(json["mode"], "auto");

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("1 files  1 symbols  1 concepts"),
        "expected graph counts line, got: {text}"
    );
}

#[test]
fn status_reports_context_export_as_optional_when_absent() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains(
            "context export: not generated (optional; synrepo export writes synrepo-context/)"
        ),
        "expected optional context export line, got: {text}"
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        json["export_freshness"],
        "not generated (optional; synrepo export writes synrepo-context/)"
    );
    assert_eq!(json["export_state"], "absent");
    assert_eq!(json["export_dir"], "synrepo-context");
}

#[test]
fn status_embedding_health_names_provider_and_source() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let mut config = Config::load(repo.path()).unwrap();
    config.enable_semantic_triage = true;
    config.semantic_embedding_provider = SemanticEmbeddingProvider::Ollama;
    config.semantic_model = "all-minilm".to_string();
    config.embedding_dim = 384;
    std::fs::write(
        Config::synrepo_dir(repo.path()).join("config.toml"),
        toml::to_string_pretty(&config).unwrap(),
    )
    .unwrap();

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["embedding_health"]["provider"], "ollama");
    assert_eq!(json["embedding_health"]["provider_source"], "explicit");
    assert_eq!(json["embedding_health"]["model"], serde_json::Value::Null);

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("ollama/all-minilm"),
        "expected provider/model in embedding line, got: {text}"
    );
}

#[test]
fn status_explain_hint_mentions_global_config_for_reusable_keys() {
    let env = EnvGuard::new();
    env.set("ANTHROPIC_API_KEY", "sk-test");
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("~/.synrepo/config.toml"),
        "expected global explain config hint, got: {text}"
    );
}

#[test]
fn status_json_reports_static_pricing_basis_without_openrouter_live_cost() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    write_explain_totals(
        repo.path(),
        &ExplainTotals {
            calls: 1,
            input_tokens: 120,
            output_tokens: 30,
            per_provider: std::iter::once((
                "openai".to_string(),
                ProviderTotals {
                    calls: 1,
                    input_tokens: 120,
                    output_tokens: 30,
                    usd_cost: Some(0.001),
                },
            ))
            .collect(),
            ..ExplainTotals::default()
        },
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        json["explain_totals"]["openrouter_live_pricing_used"],
        serde_json::json!(false)
    );
    assert_eq!(
        json["explain_totals"]["pricing_basis"],
        serde_json::json!(format!(
            "static table as of {}",
            synrepo::pipeline::explain::pricing::LAST_UPDATED
        ))
    );
}

#[test]
fn status_json_reports_live_openrouter_pricing_basis_when_openrouter_cost_present() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    write_explain_totals(
        repo.path(),
        &ExplainTotals {
            calls: 2,
            input_tokens: 300,
            output_tokens: 90,
            per_provider: std::iter::once((
                "openrouter".to_string(),
                ProviderTotals {
                    calls: 2,
                    input_tokens: 300,
                    output_tokens: 90,
                    usd_cost: Some(0.0025),
                },
            ))
            .collect(),
            ..ExplainTotals::default()
        },
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        json["explain_totals"]["openrouter_live_pricing_used"],
        serde_json::json!(true)
    );
    assert_eq!(
        json["explain_totals"]["pricing_basis"],
        serde_json::json!(format!(
            "static table as of {}; OpenRouter live",
            synrepo::pipeline::explain::pricing::LAST_UPDATED
        ))
    );
}

#[test]
#[cfg(unix)]
fn status_reports_writer_lock_held_by_other() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    // Take the kernel flock on a separate fd and stamp ownership pointing at
    // a live foreign PID. Stamping JSON without taking the flock would leave
    // `compute_writer_status` reporting Free; see CLAUDE.md writer-lock gotcha.
    let (mut child, pid) = synrepo::pipeline::writer::live_foreign_pid();
    let _flock = synrepo::pipeline::writer::hold_writer_flock_with_ownership(
        &writer_lock_path(&synrepo_dir),
        &WriterOwnership {
            pid,
            acquired_at: "now".to_string(),
        },
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        json["writer_lock"],
        serde_json::Value::String(format!("held_by_pid_{pid}")),
        "expected writer_lock held_by_pid_{pid}, full json: {json}"
    );

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains(&format!("held by pid {pid}")),
        "expected writer-lock line in text output, got: {text}"
    );
    assert!(
        text.contains("writer lock is held"),
        "expected writer-lock next-step hint, got: {text}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn status_overlay_cost_surfaces_query_failure() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let overlay_dir = synrepo_dir.join("overlay");
    let _ = SqliteOverlayStore::open(&overlay_dir).unwrap();
    let db_path = SqliteOverlayStore::db_path(&overlay_dir);
    std::fs::write(&db_path, b"this is not a sqlite database header").unwrap();

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("overlay cost: unavailable"),
        "expected overlay-cost unavailable line, got: {text}"
    );
    assert!(
        !text.contains("overlay cost: no overlay") && !text.contains("overlay cost: 0 LLM calls"),
        "overlay-cost line must not collapse a query failure to zero, got: {text}"
    );
}

#[test]
fn status_commentary_coverage_graph_unreadable() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    let node = NodeId::Symbol(SymbolNodeId(0xabc));
    overlay
        .insert_commentary(CommentaryEntry {
            node_id: node,
            text: "Test commentary entry.".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: "h1".to_string(),
                pass_id: "test-commentary-v1".to_string(),
                model_identity: "test-model".to_string(),
                generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
            },
        })
        .unwrap();

    let graph_dir = synrepo_dir.join("graph");
    std::fs::remove_dir_all(&graph_dir).unwrap();

    let text = status_output(repo.path(), false, false, true).unwrap();
    assert!(
        text.contains("commentary:   1 entries (graph unreadable)"),
        "expected `1 entries (graph unreadable)` line, got: {text}"
    );
}

#[test]
fn status_reports_capability_readiness_matrix_in_text_and_json() {
    // Scenario 3.3: status must surface the shared readiness matrix so a
    // degraded or optional feature is labeled consistently across surfaces.
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("capability readiness:"),
        "text status output must include the capability readiness section, got: {text}"
    );
    assert!(
        text.contains("embedding:    off (optional; lexical routing/search still available)"),
        "status must label disabled embeddings as optional, got: {text}"
    );
    // The seed_graph fixture does not set up embeddings; the embeddings row
    // must be visible as disabled.
    assert!(
        text.contains(
            "embeddings         disabled     optional; semantic routing uses lexical fallback"
        ),
        "readiness section must list the embeddings capability, got: {text}"
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    let matrix = json
        .get("capability_readiness")
        .expect("json status must include capability_readiness");
    assert!(
        matrix.is_array(),
        "capability_readiness must be an array, got: {matrix}"
    );
    let rows = matrix.as_array().unwrap();
    assert_eq!(rows.len(), 8, "matrix must contain eight capability rows");
    let labels: Vec<&str> = rows
        .iter()
        .map(|row| row["capability"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"parser"));
    assert!(labels.contains(&"project-layout"));
    assert!(labels.contains(&"overlay"));
    assert!(labels.contains(&"compatibility"));
}

#[test]
fn status_recent_activity_json_round_trip() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, true, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert!(
        json["recent_activity"].is_array(),
        "expected recent_activity to be an array, got: {}",
        json["recent_activity"]
    );

    let null_json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert!(
        null_json["recent_activity"].is_null(),
        "expected null recent_activity when recent=false, got: {}",
        null_json["recent_activity"]
    );
}
