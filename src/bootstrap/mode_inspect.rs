//! Repository inspection for auto vs curated mode selection.

use crate::config::{Config, Mode};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Result of inspecting a repository for rationale markdown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModeInspection {
    pub recommended_mode: Mode,
    pub rationale_dirs: Vec<PathBuf>,
}

impl ModeInspection {
    /// Produce optional user-facing guidance about the mode selection.
    pub fn guidance_for(
        &self,
        requested_mode: Option<Mode>,
        existing_config: Option<&Config>,
        final_mode: Mode,
    ) -> Option<String> {
        if self.rationale_dirs.is_empty() {
            return match (requested_mode, existing_config) {
                (None, None) => Some(
                    "no rationale markdown was found under configured concept directories, so bootstrap defaulted to Auto.".to_string(),
                ),
                _ => None,
            };
        }

        let rationale_dirs = display_paths(&self.rationale_dirs);
        match (requested_mode, existing_config) {
            (Some(explicit_mode), _) if explicit_mode != self.recommended_mode => Some(format!(
                "repository inspection suggests {:?} because rationale markdown was found in {}; keeping explicit {:?}.",
                self.recommended_mode, rationale_dirs, explicit_mode
            )),
            (None, Some(config)) if config.mode != self.recommended_mode => Some(format!(
                "repository inspection suggests {:?} because rationale markdown was found in {}; keeping configured {:?}. Rerun `synrepo init --mode {}` to switch.",
                self.recommended_mode,
                rationale_dirs,
                config.mode,
                mode_flag(self.recommended_mode)
            )),
            (None, None) if final_mode == self.recommended_mode => Some(format!(
                "repository inspection selected {:?} because rationale markdown was found in {}.",
                final_mode, rationale_dirs
            )),
            _ => None,
        }
    }
}

/// Inspect the repository for rationale markdown to recommend a mode.
pub fn inspect_repository_mode(
    repo_root: &Path,
    config: &Config,
) -> anyhow::Result<ModeInspection> {
    let rationale_dirs = config
        .concept_directories
        .iter()
        .filter_map(|relative_dir| {
            let dir = repo_root.join(relative_dir);
            if !dir.exists() {
                return None;
            }
            Some((relative_dir, dir))
        })
        .filter_map(|(relative_dir, dir)| match contains_markdown(&dir) {
            Ok(true) => Some(Ok(PathBuf::from(relative_dir))),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let recommended_mode = if rationale_dirs.is_empty() {
        Mode::Auto
    } else {
        Mode::Curated
    };

    Ok(ModeInspection {
        recommended_mode,
        rationale_dirs,
    })
}

fn contains_markdown(path: &Path) -> anyhow::Result<bool> {
    if path.is_file() {
        return Ok(is_markdown_path(path));
    }

    for entry in WalkDir::new(path) {
        let entry = entry?;
        if entry.file_type().is_file() && is_markdown_path(entry.path()) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext, "md" | "mdx" | "markdown"))
        .unwrap_or(false)
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn mode_flag(mode: Mode) -> &'static str {
    match mode {
        Mode::Auto => "auto",
        Mode::Curated => "curated",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn inspect_empty_repo_returns_auto() {
        let dir = tempdir().unwrap();
        let config = Config::default();

        let inspection = inspect_repository_mode(dir.path(), &config).unwrap();
        assert_eq!(inspection.recommended_mode, Mode::Auto);
        assert!(inspection.rationale_dirs.is_empty());
    }

    #[test]
    fn inspect_finds_markdown_in_configured_dirs() {
        let dir = tempdir().unwrap();
        let config = Config {
            concept_directories: vec!["docs".to_string()],
            ..Config::default()
        };

        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("test.md"), "test").unwrap();
        fs::write(docs_dir.join("test.mdx"), "test").unwrap();
        fs::write(docs_dir.join("test.markdown"), "test").unwrap();

        let inspection = inspect_repository_mode(dir.path(), &config).unwrap();
        assert_eq!(inspection.recommended_mode, Mode::Curated);
        assert_eq!(inspection.rationale_dirs, vec![PathBuf::from("docs")]);
    }

    #[test]
    fn inspect_ignores_non_markdown_files() {
        let dir = tempdir().unwrap();
        let config = Config {
            concept_directories: vec!["docs".to_string()],
            ..Config::default()
        };

        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("test.txt"), "txt").unwrap();
        fs::write(docs_dir.join("test.rs"), "rs").unwrap();

        let inspection = inspect_repository_mode(dir.path(), &config).unwrap();
        assert_eq!(inspection.recommended_mode, Mode::Auto);
        assert!(inspection.rationale_dirs.is_empty());
    }

    #[test]
    fn guidance_formatting_matrix() {
        let inspection = ModeInspection {
            recommended_mode: Mode::Curated,
            rationale_dirs: vec![PathBuf::from("docs/concepts")],
        };
        let config = Config {
            mode: Mode::Auto,
            ..Config::default()
        };

        // Requested explicitly mismatches recommended
        let guidance = inspection
            .guidance_for(Some(Mode::Auto), None, Mode::Auto)
            .unwrap();
        assert!(guidance.contains("suggests Curated"));
        assert!(guidance.contains("keeping explicit Auto"));

        // Existing config mismatches recommended
        let guidance = inspection
            .guidance_for(None, Some(&config), Mode::Auto)
            .unwrap();
        assert!(guidance.contains("suggests Curated"));
        assert!(guidance.contains("keeping configured Auto"));
        // Since config.mode is Auto, `mode_flag(recommended)` will format `mode_flag(Curated) -> "curated"`
        assert!(guidance.contains("--mode curated"));

        // No config, defaults to recommended
        let guidance = inspection.guidance_for(None, None, Mode::Curated).unwrap();
        assert!(guidance.contains("selected Curated"));
    }
}
