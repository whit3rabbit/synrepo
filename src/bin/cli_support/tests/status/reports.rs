use std::process::Command;

use synrepo::config::Config;
use synrepo::core::ids::{NodeId, SymbolNodeId};
use synrepo::overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore};
use synrepo::pipeline::synthesis::accounting::{ProviderTotals, SynthesisTotals};
use synrepo::pipeline::writer::{writer_lock_path, WriterOwnership};
use synrepo::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;
use time::OffsetDateTime;

use super::{seed_graph, status_output, write_synthesis_totals, EnvGuard};

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
fn status_synthesis_hint_mentions_global_config_for_reusable_keys() {
    let env = EnvGuard::new();
    env.set("ANTHROPIC_API_KEY", "sk-test");
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("~/.synrepo/config.toml"),
        "expected global synthesis config hint, got: {text}"
    );
}

#[test]
fn status_json_reports_static_pricing_basis_without_openrouter_live_cost() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    write_synthesis_totals(
        repo.path(),
        &SynthesisTotals {
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
            ..SynthesisTotals::default()
        },
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        json["synthesis_totals"]["openrouter_live_pricing_used"],
        serde_json::json!(false)
    );
    assert_eq!(
        json["synthesis_totals"]["pricing_basis"],
        serde_json::json!(format!(
            "static table as of {}",
            synrepo::pipeline::synthesis::pricing::LAST_UPDATED
        ))
    );
}

#[test]
fn status_json_reports_live_openrouter_pricing_basis_when_openrouter_cost_present() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    write_synthesis_totals(
        repo.path(),
        &SynthesisTotals {
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
            ..SynthesisTotals::default()
        },
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        json["synthesis_totals"]["openrouter_live_pricing_used"],
        serde_json::json!(true)
    );
    assert_eq!(
        json["synthesis_totals"]["pricing_basis"],
        serde_json::json!(format!(
            "static table as of {}; OpenRouter live",
            synrepo::pipeline::synthesis::pricing::LAST_UPDATED
        ))
    );
}

#[test]
fn status_reports_writer_lock_held_by_other() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let mut child = Command::new("sleep").arg("5").spawn().unwrap();
    let pid = child.id();
    std::fs::write(
        writer_lock_path(&synrepo_dir),
        serde_json::to_string(&WriterOwnership {
            pid,
            acquired_at: "now".to_string(),
        })
        .unwrap(),
    )
    .unwrap();

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
