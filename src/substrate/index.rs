//! The lexical index: builds and queries the persisted syntext index under `.synrepo/index/`.
//!
//! The corpus admitted to indexing is determined by `crate::substrate::discover`,
//! while `syntext` provides the segment format and exact-search engine.

use std::path::{Path, PathBuf};
use syntext::index::{ExternalFileRecord, Index};
use syntext::{Config as SyntextConfig, SearchOptions};

/// Summary of a persisted substrate rebuild.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexBuildReport {
    /// Number of discovered files admitted to the rebuilt index.
    pub indexed_files: usize,
}

/// Builds the current synrepo-owned substrate index.
///
/// Discovery and file admission come from `substrate::discover`. The resulting
/// discovered corpus is serialized into syntext-compatible base segments so the
/// index persists across later `search` calls and process restarts.
pub fn build_index(
    config: &crate::config::Config,
    repo_root: &Path,
) -> crate::Result<IndexBuildReport> {
    let discovered = crate::substrate::discover::discover(repo_root, config)?;
    let records = discovered
        .iter()
        .map(|file| ExternalFileRecord {
            absolute_path: file.absolute_path.clone(),
            relative_path: PathBuf::from(&file.relative_path),
            size_bytes: file.size_bytes,
        })
        .collect();
    Index::build_from_file_records(syntext_config(config, repo_root), records)
        .map_err(map_index_error)?;

    Ok(IndexBuildReport {
        indexed_files: discovered.len(),
    })
}

/// Executes an exact lexical search against the current substrate index.
pub fn search(
    config: &crate::config::Config,
    repo_root: &Path,
    query: &str,
) -> crate::Result<Vec<syntext::SearchMatch>> {
    search_with_options(config, repo_root, query, &SearchOptions::default())
}

/// Executes an exact lexical search against the current substrate index using
/// explicit syntext search options.
pub fn search_with_options(
    config: &crate::config::Config,
    repo_root: &Path,
    query: &str,
    options: &SearchOptions,
) -> crate::Result<Vec<syntext::SearchMatch>> {
    let syntext_config = syntext_config(config, repo_root);
    let manifest_path = syntext_config.index_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "substrate index is missing at {}. Run `synrepo init` first.",
            syntext_config.index_dir.display()
        )));
    }

    let index = Index::open(syntext_config).map_err(map_index_error)?;

    let results = index.search(query, options).map_err(|e| {
        crate::Error::Other(anyhow::anyhow!(
            "substrate search failed for `{query}`: {e}"
        ))
    })?;

    Ok(results)
}

fn syntext_config(config: &crate::config::Config, repo_root: &Path) -> SyntextConfig {
    SyntextConfig {
        index_dir: crate::config::Config::synrepo_dir(repo_root).join("index"),
        repo_root: repo_root.to_path_buf(),
        max_file_size: config.max_file_size_bytes,
        ..SyntextConfig::default()
    }
}

fn map_index_error(error: syntext::IndexError) -> crate::Error {
    match error {
        syntext::IndexError::CorruptIndex(message) => crate::Error::Other(anyhow::anyhow!(
            "substrate index is unusable: {message}. Re-run `synrepo init` to rebuild it."
        )),
        syntext::IndexError::LockConflict(path) => crate::Error::Other(anyhow::anyhow!(
            "substrate index at {} is locked by another process",
            path.display()
        )),
        other => crate::Error::Other(anyhow::anyhow!("unable to open substrate index: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn build_index_and_search_follow_discovery_contract() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join(".synrepo/index")).unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(repo.path().join(".gitignore"), "src/ignored.rs\n").unwrap();

        fs::write(
            repo.path().join("src/lib.rs"),
            "pub fn visible_symbol() { println!(\"visible token\"); }\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("src/ignored.rs"),
            "pub fn hidden_symbol() { println!(\"ignored token\"); }\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("docs/guide.md"),
            "# Guide\nThis file mentions visible token in docs.\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("docs/secret.env"),
            "API_TOKEN=secret token\n",
        )
        .unwrap();
        fs::write(repo.path().join("docs/blob.bin"), [0, 159, 146, 150]).unwrap();

        let config = Config::default();
        let report = build_index(&config, repo.path()).unwrap();
        assert!(report.indexed_files >= 2);

        let visible = search(&config, repo.path(), "visible token").unwrap();
        let found_paths: Vec<_> = visible
            .into_iter()
            .map(|m| m.path.to_string_lossy().into_owned())
            .collect();
        assert!(found_paths.iter().any(|path| path == "docs/guide.md"));
        assert!(found_paths.iter().any(|path| path == "src/lib.rs"));

        let ignored = search(&config, repo.path(), "ignored token").unwrap();
        assert!(ignored.is_empty());

        let redacted = search(&config, repo.path(), "secret token").unwrap();
        assert!(redacted.is_empty());
    }

    #[test]
    fn search_fails_clearly_when_index_is_missing() {
        let repo = tempdir().unwrap();
        let config = Config::default();

        let error = search(&config, repo.path(), "anything").unwrap_err();
        let message = error.to_string();

        assert!(message.contains("substrate index is missing"));
        assert!(message.contains("synrepo init"));
    }

    #[test]
    fn search_with_options_respects_syntext_filters() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join(".synrepo/index")).unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::create_dir_all(repo.path().join("tests")).unwrap();

        fs::write(
            repo.path().join("src/lib.rs"),
            "pub fn visible_symbol() { println!(\"Visible Token\"); }\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("tests/lib_test.rs"),
            "fn test_visible() { println!(\"visible token\"); }\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("src/helper.py"),
            "print('visible token from python')\n",
        )
        .unwrap();

        let config = Config::default();
        build_index(&config, repo.path()).unwrap();

        let options = SearchOptions {
            path_filter: Some("src/".to_string()),
            file_type: Some("rs".to_string()),
            max_results: Some(1),
            case_insensitive: true,
            ..SearchOptions::default()
        };
        let matches = search_with_options(&config, repo.path(), "visible token", &options).unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, Path::new("src/lib.rs"));
    }
}
