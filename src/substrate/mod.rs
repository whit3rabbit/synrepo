//! The lexical substrate layer.
//!
//! Builds and queries the persisted lexical index under `.synrepo/index/`.
//! The corpus admitted to indexing is selected by synrepo's own discovery
//! contract in `crate::structure::discover`, while `syntext` provides the
//! segment format and exact-search engine.

use std::{fs, path::Path};
use syntext::index::manifest::{Manifest, SegmentRef};
use syntext::index::segment::SegmentWriter;
use syntext::index::Index;
use syntext::tokenizer::build_all;
use syntext::{Config as SyntextConfig, SearchOptions};

const SEGMENT_BATCH_SIZE_BYTES: u64 = 256 * 1024 * 1024;

/// Summary of a persisted substrate rebuild.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexBuildReport {
    /// Number of discovered files admitted to the rebuilt index.
    pub indexed_files: usize,
}

/// Builds the current synrepo-owned substrate index.
///
/// Discovery and file admission come from `structure::discover`. The resulting
/// discovered corpus is serialized into syntext-compatible base segments so the
/// index persists across later `search` calls and process restarts.
pub fn build_index(
    config: &crate::config::Config,
    repo_root: &Path,
) -> crate::Result<IndexBuildReport> {
    let syntext_config = syntext_config(config, repo_root);
    let index_dir = syntext_config.index_dir.clone();
    fs::create_dir_all(&index_dir)?;
    ensure_index_dir_permissions(&index_dir)?;
    clear_existing_index_artifacts(&index_dir)?;

    let discovered = crate::structure::discover::discover(repo_root, config)?;
    let segment_refs = write_discovered_segments(&discovered, &index_dir)?;
    let total_files_indexed = u32::try_from(discovered.len())
        .map_err(|_| crate::Error::Other(anyhow::anyhow!("too many discovered files to index")))?;
    let manifest = Manifest::new(segment_refs, total_files_indexed);
    manifest.save(&index_dir)?;

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
    let syntext_config = syntext_config(config, repo_root);
    let manifest_path = syntext_config.index_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "substrate index is missing at {}. Run `synrepo init` first.",
            syntext_config.index_dir.display()
        )));
    }

    let index = Index::open(syntext_config).map_err(map_open_error)?;

    let results = index
        .search(query, &SearchOptions::default())
        .map_err(|e| {
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

fn clear_existing_index_artifacts(index_dir: &Path) -> crate::Result<()> {
    if !index_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(index_dir)? {
        let entry = entry?;
        let path = entry.path();
        let should_remove = matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("dict" | "post" | "seg" | "tmp")
        ) || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "manifest.json");

        if should_remove {
            fs::remove_file(path)?;
        }
    }

    Ok(())
}

fn write_discovered_segments(
    discovered: &[crate::structure::discover::DiscoveredFile],
    index_dir: &Path,
) -> crate::Result<Vec<SegmentRef>> {
    let mut segment_refs = Vec::new();
    let mut writer = SegmentWriter::new();
    let mut batch_bytes = 0_u64;
    let mut next_doc_id = 0_u32;

    for file in discovered {
        if writer.doc_count() > 0
            && batch_bytes.saturating_add(file.size_bytes) > SEGMENT_BATCH_SIZE_BYTES
        {
            segment_refs.push(flush_segment(writer, index_dir)?);
            writer = SegmentWriter::new();
            batch_bytes = 0;
        }

        let content = fs::read(&file.absolute_path)?;
        let content_hash = content_hash(&content);
        let doc_id = next_doc_id;
        next_doc_id = next_doc_id.checked_add(1).ok_or_else(|| {
            crate::Error::Other(anyhow::anyhow!("doc_id overflow during index build"))
        })?;

        let relative_path = Path::new(&file.relative_path);
        writer.add_document(doc_id, relative_path, content_hash, content.len() as u64);
        for gram_hash in build_all(&content) {
            writer.add_gram_posting(gram_hash, doc_id);
        }

        let normalized_size = u64::try_from(content.len())
            .map_err(|_| crate::Error::Other(anyhow::anyhow!("indexed file size overflow")))?;
        batch_bytes = batch_bytes.saturating_add(normalized_size);
    }

    if writer.doc_count() > 0 {
        segment_refs.push(flush_segment(writer, index_dir)?);
    }

    Ok(segment_refs)
}

fn flush_segment(writer: SegmentWriter, index_dir: &Path) -> crate::Result<SegmentRef> {
    let meta = writer.write_to_dir(index_dir)?;
    Ok(SegmentRef::from(meta))
}

fn content_hash(bytes: &[u8]) -> u64 {
    let digest = blake3::hash(bytes);
    let mut first_eight = [0_u8; 8];
    first_eight.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_le_bytes(first_eight)
}

fn map_open_error(error: syntext::IndexError) -> crate::Error {
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

#[cfg(unix)]
fn ensure_index_dir_permissions(index_dir: &Path) -> crate::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(index_dir, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn ensure_index_dir_permissions(_index_dir: &Path) -> crate::Result<()> {
    Ok(())
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
}
