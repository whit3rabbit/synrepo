//! The lexical index: builds and queries the persisted syntext index under `.synrepo/index/`.
//!
//! The corpus admitted to indexing is determined by `crate::substrate::discover`,
//! while `syntext` provides the segment format and exact-search engine.

use globset::Glob;
use std::path::{Path, PathBuf};
use syntext::index::{ExternalFileRecord, Index};
use syntext::{Config as SyntextConfig, SearchOptions};

const GLOB_FILTER_OVERFETCH_LIMIT: usize = 10_000;
const GLOB_FILTER_OVERFETCH_MULTIPLIER: usize = 200;

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
    build_index_with_retry(config, repo_root)
}

fn build_index_with_retry(
    config: &crate::config::Config,
    repo_root: &Path,
) -> crate::Result<IndexBuildReport> {
    match build_index_once(config, repo_root) {
        Ok(report) => Ok(report),
        Err(err @ crate::Error::Other(_)) if is_lock_conflict(&err) => {
            std::thread::sleep(std::time::Duration::from_millis(20));
            match build_index_once(config, repo_root) {
                Ok(report) => Ok(report),
                Err(retry_err @ crate::Error::Other(_)) if is_lock_conflict(&retry_err) => {
                    let index_dir = crate::config::Config::synrepo_dir(repo_root).join("index");
                    let _ = std::fs::remove_dir_all(&index_dir);
                    std::fs::create_dir_all(&index_dir)?;
                    build_index_once(config, repo_root)
                }
                Err(retry_err) => Err(retry_err),
            }
        }
        Err(err) => Err(err),
    }
}

fn build_index_once(
    config: &crate::config::Config,
    repo_root: &Path,
) -> crate::Result<IndexBuildReport> {
    let discovered = crate::substrate::discover::discover(repo_root, config)?;
    let records = discovered
        .iter()
        .filter(|file| file.root_discriminant == "primary")
        .map(|file| ExternalFileRecord {
            absolute_path: file.absolute_path.clone(),
            relative_path: PathBuf::from(&file.relative_path),
            size_bytes: file.size_bytes,
        })
        .collect::<Vec<_>>();
    let indexed_files = records.len();
    Index::build_from_file_records(syntext_config(config, repo_root), records)
        .map_err(map_index_error)?;

    Ok(IndexBuildReport { indexed_files })
}

fn is_lock_conflict(error: &crate::Error) -> bool {
    matches!(error, crate::Error::Other(err) if err.to_string().contains("locked by another process"))
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
///
/// If `options.path_filter` contains glob patterns, this function performs
/// a two-stage match: it queries the index using the longest non-glob prefix
/// of the filter, then refines the results using a glob matcher.
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

    // Prepare glob matcher and index prefix if filter is present
    let (prefix, matcher) = if let Some(filter) = &options.path_filter {
        if contains_glob_chars(filter) {
            let prefix = extract_non_glob_prefix(filter);
            let glob = Glob::new(filter).map_err(|e| {
                crate::Error::Other(anyhow::anyhow!(
                    "invalid path filter glob `{}`: {}",
                    filter,
                    e
                ))
            })?;
            (
                if prefix.is_empty() {
                    None
                } else {
                    Some(prefix)
                },
                Some(glob.compile_matcher()),
            )
        } else {
            // No glob characters: treat as a simple directory/path prefix (legacy behavior)
            (Some(filter.clone()), None)
        }
    } else {
        (None, None)
    };

    // Glob filters need overfetch so valid post-filter matches are not clipped
    // too early, but the index phase must still be bounded on broad globs.
    let effective_options = SearchOptions {
        path_filter: prefix,
        file_type: options.file_type.clone(),
        exclude_type: options.exclude_type.clone(),
        max_results: if matcher.is_some() {
            Some(glob_filter_overfetch_limit(options.max_results))
        } else {
            options.max_results
        },
        case_insensitive: options.case_insensitive,
    };

    let index = Index::open(syntext_config).map_err(map_index_error)?;

    let mut results = index.search(query, &effective_options).map_err(|e| {
        crate::Error::Other(anyhow::anyhow!(
            "substrate search failed for `{}`: {}",
            query,
            e
        ))
    })?;

    // Apply post-filtering if a glob was provided
    if let Some(matcher) = matcher {
        results.retain(|m| matcher.is_match(&m.path));
        if let Some(limit) = options.max_results {
            results.truncate(limit);
        }
    }

    Ok(results)
}

fn extract_non_glob_prefix(filter: &str) -> String {
    let mut last_slash = 0;
    for (i, c) in filter.char_indices() {
        if c == '*' || c == '?' || c == '[' || c == '{' {
            return filter[..last_slash].to_string();
        }
        if c == '/' {
            last_slash = i + 1;
        }
    }
    filter.to_string()
}

fn contains_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}

fn glob_filter_overfetch_limit(max_results: Option<usize>) -> usize {
    match max_results {
        Some(limit) => limit
            .saturating_mul(GLOB_FILTER_OVERFETCH_MULTIPLIER)
            .min(GLOB_FILTER_OVERFETCH_LIMIT),
        None => GLOB_FILTER_OVERFETCH_LIMIT,
    }
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

    #[test]
    fn search_with_options_respects_glob_filters() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join(".synrepo/index")).unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::create_dir_all(repo.path().join("tests")).unwrap();

        fs::write(
            repo.path().join("src/lib.rs"),
            "pub fn visible() { println!(\"token\"); }\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("tests/lib_test.py"),
            "def test_visible(): print(\"token\")\n",
        )
        .unwrap();

        let config = Config::default();
        build_index(&config, repo.path()).unwrap();

        // 1. Matches both via prefix
        let options_no_glob = SearchOptions {
            path_filter: None,
            ..SearchOptions::default()
        };
        let matches = search_with_options(&config, repo.path(), "token", &options_no_glob).unwrap();
        assert_eq!(matches.len(), 2);

        // 2. Matches only .rs via glob
        let options_glob = SearchOptions {
            path_filter: Some("**/*.rs".to_string()),
            ..SearchOptions::default()
        };
        let matches_glob =
            search_with_options(&config, repo.path(), "token", &options_glob).unwrap();
        assert_eq!(matches_glob.len(), 1);
        assert_eq!(matches_glob[0].path, Path::new("src/lib.rs"));

        // 3. Matches only tests/ via prefix + glob
        let options_prefix_glob = SearchOptions {
            path_filter: Some("tests/*.py".to_string()),
            ..SearchOptions::default()
        };
        let matches_prefix_glob =
            search_with_options(&config, repo.path(), "token", &options_prefix_glob).unwrap();
        assert_eq!(matches_prefix_glob.len(), 1);
        assert_eq!(matches_prefix_glob[0].path, Path::new("tests/lib_test.py"));
    }

    #[test]
    fn test_extract_non_glob_prefix() {
        assert_eq!(extract_non_glob_prefix("src/lib.rs"), "src/lib.rs");
        assert_eq!(extract_non_glob_prefix("src/*.rs"), "src/");
        assert_eq!(extract_non_glob_prefix("src/**/*.rs"), "src/");
        assert_eq!(extract_non_glob_prefix("**/src/*.rs"), "");
        assert_eq!(extract_non_glob_prefix("docs/"), "docs/");
        assert_eq!(glob_filter_overfetch_limit(Some(10)), 2_000);
        assert_eq!(glob_filter_overfetch_limit(Some(10_000)), 10_000);
    }
}
