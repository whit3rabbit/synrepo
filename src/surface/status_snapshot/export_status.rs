//! Export status derivation for status and dashboard surfaces.

use std::path::Path;

use crate::{
    config::Config,
    pipeline::{export::load_manifest, watch::load_reconcile_state},
};

/// Structured state for the optional context export surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportState {
    /// No export manifest exists. This is healthy because exports are optional.
    Absent,
    /// The export manifest matches the latest reconcile epoch.
    Current,
    /// The graph has advanced since the export manifest was written.
    Stale,
}

impl ExportState {
    /// Stable lowercase tag used by JSON status output.
    pub fn as_str(self) -> &'static str {
        match self {
            ExportState::Absent => "absent",
            ExportState::Current => "current",
            ExportState::Stale => "stale",
        }
    }
}

/// Display-ready context export status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportStatus {
    /// Structured export state.
    pub state: ExportState,
    /// Human-readable summary line.
    pub display: String,
    /// Configured export directory, relative to the repo root.
    pub export_dir: String,
    /// Export format from the manifest, when one exists.
    pub format: Option<String>,
    /// Export card budget from the manifest, when one exists.
    pub budget: Option<String>,
}

/// Build export status without mutating the export directory.
pub fn build_export_status(repo_root: &Path, synrepo_dir: &Path, config: &Config) -> ExportStatus {
    let export_dir = config.export_dir.clone();
    let display_dir = export_dir_with_slash(&export_dir);
    let Some(manifest) = load_manifest(repo_root, config) else {
        return ExportStatus {
            state: ExportState::Absent,
            display: format!("not generated (optional; synrepo export writes {display_dir})"),
            export_dir,
            format: None,
            budget: None,
        };
    };

    let format = manifest.format.as_str().to_string();
    let budget = manifest.budget.clone();
    let current_epoch = load_reconcile_state(synrepo_dir)
        .map(|r| r.last_reconcile_at)
        .unwrap_or_default();

    if manifest.last_reconcile_at == current_epoch {
        ExportStatus {
            state: ExportState::Current,
            display: format!("current ({format}, {budget})"),
            export_dir,
            format: Some(format),
            budget: Some(budget),
        }
    } else {
        ExportStatus {
            state: ExportState::Stale,
            display: format!(
                "stale (generated at {}, current epoch {})",
                manifest.last_reconcile_at, current_epoch
            ),
            export_dir,
            format: Some(format),
            budget: Some(budget),
        }
    }
}

fn export_dir_with_slash(export_dir: &str) -> String {
    if export_dir.ends_with('/') {
        export_dir.to_string()
    } else {
        format!("{export_dir}/")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::{
        config::Config,
        pipeline::{
            export::{ExportFormat, ExportManifest, MANIFEST_FILENAME},
            watch::{persist_reconcile_state, ReconcileOutcome},
        },
        store::compatibility::GRAPH_FORMAT_VERSION,
    };

    use super::{build_export_status, ExportState};

    #[test]
    fn absent_export_is_optional() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        let config = Config::default();
        let synrepo_dir = repo.join(".synrepo");

        let status = build_export_status(repo, &synrepo_dir, &config);

        assert_eq!(status.state, ExportState::Absent);
        assert_eq!(
            status.display,
            "not generated (optional; synrepo export writes synrepo-context/)"
        );
        assert_eq!(status.export_dir, "synrepo-context");
        assert_eq!(status.format, None);
        assert_eq!(status.budget, None);
    }

    #[test]
    fn current_export_keeps_manifest_shape() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        let config = Config::default();
        let synrepo_dir = repo.join(".synrepo");
        persist_reconcile_state(
            &synrepo_dir,
            &ReconcileOutcome::Completed(Default::default()),
            0,
        );
        let epoch = crate::pipeline::watch::load_reconcile_state(&synrepo_dir)
            .unwrap()
            .last_reconcile_at;
        write_manifest(repo, &config, &epoch, ExportFormat::Markdown, "normal");

        let status = build_export_status(repo, &synrepo_dir, &config);

        assert_eq!(status.state, ExportState::Current);
        assert_eq!(status.display, "current (markdown, normal)");
        assert_eq!(status.format.as_deref(), Some("markdown"));
        assert_eq!(status.budget.as_deref(), Some("normal"));
    }

    #[test]
    fn stale_export_reports_epoch_delta() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        let config = Config::default();
        let synrepo_dir = repo.join(".synrepo");
        persist_reconcile_state(
            &synrepo_dir,
            &ReconcileOutcome::Completed(Default::default()),
            0,
        );
        write_manifest(repo, &config, "old-epoch", ExportFormat::Json, "deep");

        let status = build_export_status(repo, &synrepo_dir, &config);

        assert_eq!(status.state, ExportState::Stale);
        assert!(status.display.starts_with("stale (generated at old-epoch"));
        assert!(status.display.contains("current epoch"));
        assert_eq!(status.format.as_deref(), Some("json"));
        assert_eq!(status.budget.as_deref(), Some("deep"));
    }

    fn write_manifest(
        repo: &std::path::Path,
        config: &Config,
        last_reconcile_at: &str,
        format: ExportFormat,
        budget: &str,
    ) {
        let export_dir = repo.join(&config.export_dir);
        fs::create_dir_all(&export_dir).unwrap();
        let manifest = ExportManifest {
            graph_schema_version: GRAPH_FORMAT_VERSION,
            last_reconcile_at: last_reconcile_at.to_string(),
            format,
            budget: budget.to_string(),
            generated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        fs::write(
            export_dir.join(MANIFEST_FILENAME),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }
}
