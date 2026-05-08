use std::sync::Mutex;

use synrepo::config::Config;
use synrepo::core::ids::NodeId;
use synrepo::overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore};
use synrepo::pipeline::explain::accounting::{self, ExplainTotals};
use synrepo::store::overlay::SqliteOverlayStore;
use time::OffsetDateTime;

static ENV_LOCK: Mutex<()> = Mutex::new(());

pub(super) struct EnvGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    pub(super) fn new() -> Self {
        let guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::remove_var("ANTHROPIC_API_KEY");
        Self { _guard: guard }
    }

    pub(super) fn set(&self, key: &str, value: &str) {
        std::env::set_var(key, value);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        std::env::remove_var("ANTHROPIC_API_KEY");
    }
}

pub(super) fn insert_commentary_row(store: &mut SqliteOverlayStore, node: NodeId, hash: &str) {
    store
        .insert_commentary(CommentaryEntry {
            node_id: node,
            text: "test commentary".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: hash.to_string(),
                pass_id: "test-commentary-v1".to_string(),
                model_identity: "test-model".to_string(),
                generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
            },
        })
        .unwrap();
}

pub(super) fn write_malformed_config(repo: &std::path::Path) {
    let synrepo_dir = Config::synrepo_dir(repo);
    std::fs::create_dir_all(&synrepo_dir).unwrap();
    std::fs::write(synrepo_dir.join("config.toml"), "not = valid = toml").unwrap();
}

pub(super) fn write_explain_totals(repo: &std::path::Path, totals: &ExplainTotals) {
    let synrepo_dir = Config::synrepo_dir(repo);
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    std::fs::write(
        accounting::totals_path(&synrepo_dir),
        serde_json::to_vec_pretty(totals).unwrap(),
    )
    .unwrap();
}

mod agent_integrations;
mod freshness;
mod initialization;
mod repair_audit;
mod reports;
mod routing;

pub(super) use super::super::commands::status_output;
pub(super) use super::support::seed_graph;
