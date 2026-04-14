//! Export pipeline: compile card state to static markdown or JSON snapshots.
//!
//! Exports are convenience surfaces produced by `synrepo export`. They are
//! never used as synthesis input (invariant 2). The export directory
//! (`synrepo-context/` by default) contains:
//! - `symbols.md` / `files.md` / `decisions.md` (markdown format)
//! - `index.json` (JSON format, all card types in one file)
//! - `.export-manifest.json` (metadata: format, budget, timestamp)
//!
//! The manifest is consumed by the repair loop (`ExportSurface`) to detect
//! stale exports.

pub mod render;

#[cfg(test)]
mod tests;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    config::Config,
    pipeline::watch::load_reconcile_state,
    store::sqlite::SqliteGraphStore,
    structure::graph::with_graph_read_snapshot,
    surface::card::{compiler::GraphCardCompiler, Budget, CardCompiler, FileCard, SymbolCard},
};

/// File written inside the export directory to track manifest freshness.
pub const MANIFEST_FILENAME: &str = ".export-manifest.json";

/// Format of generated export files.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// One markdown file per card type.
    Markdown,
    /// Single `index.json` file with all card types.
    Json,
}

impl ExportFormat {
    /// Stable string used for display and serialization.
    pub fn as_str(self) -> &'static str {
        match self {
            ExportFormat::Markdown => "markdown",
            ExportFormat::Json => "json",
        }
    }
}

/// Manifest written to `.export-manifest.json` inside the export directory.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportManifest {
    /// Graph store schema version at export time.
    pub graph_schema_version: u32,
    /// `last_reconcile_at` from `ReconcileState` at export time. Used to
    /// determine staleness: if the current reconcile timestamp differs,
    /// the export is stale.
    pub last_reconcile_at: String,
    /// Export format used.
    pub format: ExportFormat,
    /// Budget tier used (`"normal"` or `"deep"`).
    pub budget: String,
    /// RFC 3339 UTC timestamp when the export was generated.
    pub generated_at: String,
}

/// Simplified decision record for export output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportDecision {
    /// Path of the source markdown file, relative to repo root.
    pub path: String,
    /// Human-authored title.
    pub title: String,
    /// Decision status from frontmatter.
    pub status: Option<String>,
    /// Summary extracted from the document.
    pub summary: Option<String>,
    /// Body text of the decision section.
    pub decision_body: Option<String>,
}

/// Result of a successful `write_exports` call.
pub struct ExportResult {
    /// Manifest written to the export directory.
    pub manifest: ExportManifest,
    /// Number of file cards written.
    pub file_count: usize,
    /// Number of symbol cards written.
    pub symbol_count: usize,
    /// Number of decision records written.
    pub decision_count: usize,
    /// Path of the export directory.
    pub export_dir: std::path::PathBuf,
}

/// Compile all cards and write them to the export directory.
///
/// Opens the graph store from `synrepo_dir`, compiles `FileCard` and
/// `SymbolCard` records at `budget`, writes rendered output to
/// `<repo_root>/<config.export_dir>/`, and writes the manifest.
pub fn write_exports(
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    format: ExportFormat,
    budget: Budget,
    commit: bool,
) -> crate::Result<ExportResult> {
    let graph_dir = synrepo_dir.join("graph");
    let graph = SqliteGraphStore::open_existing(&graph_dir)?;
    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo_root)).with_config(config.clone());

    // Collect all node IDs under a single snapshot epoch.
    let (file_ids, symbol_ids, concept_ids) = with_graph_read_snapshot(compiler.graph(), |g| {
        let file_ids: Vec<_> = g.all_file_paths()?.into_iter().map(|(_, id)| id).collect();
        let symbol_ids: Vec<_> = g
            .all_symbol_names()?
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let concept_ids: Vec<_> = g
            .all_concept_paths()?
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        Ok((file_ids, symbol_ids, concept_ids))
    })?;

    // Compile file cards (each call opens its own snapshot).
    let mut file_cards: Vec<FileCard> = Vec::with_capacity(file_ids.len());
    for id in &file_ids {
        match compiler.file_card(*id, budget) {
            Ok(card) => file_cards.push(card),
            Err(err) => {
                tracing::warn!(id = ?id, error = %err, "export: skipping unreadable file card");
            }
        }
    }

    // Compile symbol cards.
    let mut symbol_cards: Vec<SymbolCard> = Vec::with_capacity(symbol_ids.len());
    for id in &symbol_ids {
        match compiler.symbol_card(*id, budget) {
            Ok(card) => symbol_cards.push(card),
            Err(err) => {
                tracing::warn!(id = ?id, error = %err, "export: skipping unreadable symbol card");
            }
        }
    }

    // Build decision records from concept nodes.
    let mut decisions: Vec<ExportDecision> = Vec::new();
    {
        let graph2 = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;
        for id in &concept_ids {
            if let Ok(Some(concept)) = with_graph_read_snapshot(&graph2, |g| g.get_concept(*id)) {
                decisions.push(ExportDecision {
                    path: concept.path.clone(),
                    title: concept.title.clone(),
                    status: concept.status.clone(),
                    summary: concept.summary.clone(),
                    decision_body: concept.decision_body.clone(),
                });
            }
        }
    }

    let export_dir = repo_root.join(&config.export_dir);
    std::fs::create_dir_all(&export_dir)?;

    let file_count = file_cards.len();
    let symbol_count = symbol_cards.len();
    let decision_count = decisions.len();

    match format {
        ExportFormat::Markdown => {
            render::write_markdown(&export_dir, &file_cards, &symbol_cards, &decisions)?;
        }
        ExportFormat::Json => {
            render::write_json(&export_dir, &file_cards, &symbol_cards, &decisions)?;
        }
    }

    // Manage .gitignore: append <export_dir>/ unless --commit is set.
    if !commit {
        ensure_gitignored(repo_root, &config.export_dir)?;
    }

    let last_reconcile_at = load_reconcile_state(synrepo_dir)
        .map(|s| s.last_reconcile_at)
        .unwrap_or_default();

    let manifest = ExportManifest {
        graph_schema_version: crate::store::compatibility::STORE_FORMAT_VERSION,
        last_reconcile_at,
        format,
        budget: format.budget_str(budget),
        generated_at: crate::pipeline::writer::now_rfc3339(),
    };

    let manifest_path = export_dir.join(MANIFEST_FILENAME);
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("manifest serialize failed: {e}")))?;
    std::fs::write(&manifest_path, manifest_json.as_bytes())?;

    Ok(ExportResult {
        manifest,
        file_count,
        symbol_count,
        decision_count,
        export_dir,
    })
}

impl ExportFormat {
    fn budget_str(self, budget: Budget) -> String {
        match budget {
            Budget::Tiny => "tiny",
            Budget::Normal => "normal",
            Budget::Deep => "deep",
        }
        .to_string()
    }
}

/// Load the export manifest from the export directory, if it exists.
pub fn load_manifest(repo_root: &Path, config: &Config) -> Option<ExportManifest> {
    let path = repo_root.join(&config.export_dir).join(MANIFEST_FILENAME);
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Append `<export_dir>/` to the repo-root `.gitignore` if not already present.
fn ensure_gitignored(repo_root: &Path, export_dir: &str) -> crate::Result<()> {
    let gitignore_path = repo_root.join(".gitignore");
    let entry = format!("{export_dir}/");

    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;
        if content.lines().any(|l| {
            let t = l.trim();
            t == entry
                || t == export_dir
                || t == format!("/{entry}")
                || t == format!("/{export_dir}")
        }) {
            return Ok(());
        }
        let append = if content.ends_with('\n') {
            format!("{entry}\n")
        } else {
            format!("\n{entry}\n")
        };
        let mut existing = content;
        existing.push_str(&append);
        std::fs::write(&gitignore_path, existing.as_bytes())?;
    } else {
        std::fs::write(&gitignore_path, format!("{entry}\n").as_bytes())?;
    }
    Ok(())
}
