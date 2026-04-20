//! Filesystem discovery: walk the configured roots and classify files.
//!
//! Respects `.gitignore`, `.git/info/exclude`, and synrepo's own `.synignore`.

use crate::config::Config;
use ignore::{
    gitignore::{Gitignore, GitignoreBuilder},
    WalkBuilder,
};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use super::classify::{classify_candidate, FileClass, SNIFF_HEAD_BYTES};

/// A file that the discovery pass decided is worth processing.
#[derive(Clone, Debug)]
pub struct DiscoveredFile {
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// Path relative to the repo root.
    pub relative_path: String,
    /// Classification.
    pub class: FileClass,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// Walk the configured roots and yield classified files.
///
/// Phase 0 implementation: honors `.gitignore` via the `ignore` crate,
/// applies size cap, applies redaction globs, sniffs encoding. Does not
/// yet integrate with git worktrees or submodules, which remains phase 1.
pub fn discover(repo_root: &Path, config: &Config) -> crate::Result<Vec<DiscoveredFile>> {
    let redaction_matcher = build_redaction_matcher(repo_root, &config.redact_globs)?;
    let mut discovered = BTreeMap::new();
    let mut walker = WalkBuilder::new(repo_root);
    walker.hidden(false);
    walker.git_ignore(true);
    walker.git_exclude(true);
    walker.git_global(true);
    walker.require_git(false);
    walker.follow_links(false);
    walker.add_custom_ignore_filename(".synignore");
    // Never descend into synrepo's own runtime state. Always-on, independent of
    // `.synrepo/.gitignore` being present. Beyond closing the feedback loop
    // (indexing our own graph output), this also avoids reading SQLite's WAL
    // sidecar files (nodes.db-shm / -wal) which hold mandatory byte-range
    // locks on Windows and would trip ERROR_LOCK_VIOLATION during sniffing.
    walker.filter_entry(|entry| entry.file_name() != ".synrepo");

    for result in walker.build() {
        let entry = match result {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }

        let absolute_path = entry.into_path();
        let relative_path = match absolute_path.strip_prefix(repo_root) {
            Ok(path) => path.to_path_buf(),
            Err(_) => continue,
        };
        if !is_within_configured_roots(&relative_path, &config.roots) {
            continue;
        }

        let size_bytes = match absolute_path.metadata() {
            Ok(metadata) => metadata.len(),
            Err(_) => continue,
        };
        let is_redacted = redaction_matcher
            .matched_path_or_any_parents(&relative_path, false)
            .is_ignore();

        let class = if size_bytes > config.max_file_size_bytes || is_redacted {
            classify_candidate(&relative_path, size_bytes, &[], config, is_redacted)
        } else {
            let first_bytes = read_file_head(&absolute_path)?;
            classify_candidate(
                &relative_path,
                size_bytes,
                &first_bytes,
                config,
                is_redacted,
            )
        };

        if matches!(class, FileClass::Skipped(_)) {
            continue;
        }

        let relative_path = normalize_relative_path(&relative_path);
        discovered
            .entry(relative_path.clone())
            .or_insert(DiscoveredFile {
                absolute_path,
                relative_path,
                class,
                size_bytes,
            });
    }

    Ok(discovered.into_values().collect())
}

fn build_redaction_matcher(repo_root: &Path, globs: &[String]) -> crate::Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(repo_root);
    for glob in globs {
        builder.add_line(None, glob).map_err(|err| {
            crate::Error::Config(format!("invalid redaction glob `{glob}`: {err}"))
        })?;
    }
    builder
        .build()
        .map_err(|err| crate::Error::Config(format!("invalid redaction matcher: {err}")))
}

fn read_file_head(path: &Path) -> crate::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0_u8; SNIFF_HEAD_BYTES];
    let bytes_read = file.read(&mut buffer)?;
    buffer.truncate(bytes_read);
    Ok(buffer)
}

fn normalize_relative_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn is_within_configured_roots(path: &Path, roots: &[String]) -> bool {
    roots.iter().any(|root| {
        if root == "." || root.is_empty() {
            return true;
        }
        let root_path = Path::new(root);
        path == root_path || path.starts_with(root_path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn discover_respects_roots_gitignore_and_redaction() {
        let repo = tempdir().unwrap();
        fs::write(repo.path().join(".gitignore"), "src/ignored.rs\n").unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::create_dir_all(repo.path().join("docs")).unwrap();

        fs::write(repo.path().join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
        fs::write(repo.path().join("src/ignored.rs"), "pub fn ignored() {}\n").unwrap();
        fs::write(repo.path().join("docs/guide.md"), "# guide\n").unwrap();
        fs::write(repo.path().join("docs/app.env"), "SECRET=1\n").unwrap();
        fs::write(repo.path().join("top.txt"), "outside configured roots\n").unwrap();

        let config = Config {
            roots: vec!["src".to_string(), "docs".to_string()],
            ..Config::default()
        };

        let discovered = discover(repo.path(), &config).unwrap();
        let relative_paths: Vec<_> = discovered
            .into_iter()
            .map(|file| file.relative_path)
            .collect();

        assert_eq!(
            relative_paths,
            vec!["docs/guide.md".to_string(), "src/lib.rs".to_string()]
        );
    }

    #[test]
    fn discover_never_walks_into_synrepo_runtime_state() {
        // Regression guard: substrate::discover must never descend into
        // `.synrepo/`, independent of whether `.synrepo/.gitignore` exists.
        // On Windows, SQLite's WAL sidecar files (`nodes.db-shm`, `nodes.db-wal`)
        // hold mandatory byte-range locks; a sniffer read into those bytes
        // returns ERROR_LOCK_VIOLATION (os error 33), which surfaced as every
        // Windows-only reconcile test failure before this filter landed.
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn real_code() {}\n").unwrap();

        // Simulate an un-bootstrapped `.synrepo/` holding files that would
        // otherwise look indexable (plain text, below size cap, not redacted).
        fs::create_dir_all(repo.path().join(".synrepo/graph")).unwrap();
        fs::write(
            repo.path().join(".synrepo/graph/nodes.db"),
            "SQLite format 3\0",
        )
        .unwrap();
        fs::write(
            repo.path().join(".synrepo/config.toml"),
            "mode = \"auto\"\n",
        )
        .unwrap();

        let discovered = discover(repo.path(), &Config::default()).unwrap();
        let paths: Vec<_> = discovered
            .iter()
            .map(|f| f.relative_path.as_str())
            .collect();
        assert!(
            paths.iter().all(|p| !p.starts_with(".synrepo")),
            "discover must skip .synrepo/ unconditionally, got: {paths:?}"
        );
        assert!(
            paths.contains(&"src/lib.rs"),
            "discover must still pick up real repo content, got: {paths:?}"
        );
    }

    #[test]
    fn discover_skips_non_text_and_oversized_files() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();

        fs::write(repo.path().join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
        fs::write(repo.path().join("src/blob.bin"), [0, 159, 146, 150]).unwrap();
        fs::write(repo.path().join("src/empty.txt"), "").unwrap();
        fs::write(
            repo.path().join("src/big.txt"),
            "abcdefghijklmnopqrstuvwxyz",
        )
        .unwrap();

        let config = Config {
            max_file_size_bytes: 20,
            ..Config::default()
        };

        let discovered = discover(repo.path(), &config).unwrap();
        let relative_paths: Vec<_> = discovered
            .into_iter()
            .map(|file| file.relative_path)
            .collect();

        assert_eq!(relative_paths, vec!["src/lib.rs".to_string()]);
    }
}
