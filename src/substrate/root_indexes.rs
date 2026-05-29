//! Per-root syntext indexes for read-only branch snapshot roots.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use globset::Glob;
use syntext::{
    index::{ExternalFileRecord, Index},
    Config as SyntextConfig, SearchOptions,
};

use crate::substrate::{
    DiscoveredFile, DiscoveryRoot, DiscoveryRootKind, RootMetadata, RootedSearchMatch,
};

pub(crate) fn build_branch_indexes(
    config: &crate::config::Config,
    repo_root: &Path,
    discovered: &[DiscoveredFile],
) -> crate::Result<usize> {
    let roots_dir = roots_index_dir(repo_root);
    let _ = std::fs::remove_dir_all(&roots_dir);
    let mut grouped = BTreeMap::<String, Vec<ExternalFileRecord>>::new();
    for file in discovered
        .iter()
        .filter(|file| file.root_kind == DiscoveryRootKind::BranchRef)
    {
        grouped
            .entry(file.root_discriminant.clone())
            .or_default()
            .push(ExternalFileRecord {
                absolute_path: file.absolute_path.clone(),
                relative_path: PathBuf::from(&file.relative_path),
                size_bytes: file.size_bytes,
            });
    }

    let root_paths = crate::substrate::discover_roots(repo_root, config)
        .into_iter()
        .filter(|root| root.kind == DiscoveryRootKind::BranchRef)
        .map(|root| (root.discriminant, root.absolute_path))
        .collect::<BTreeMap<_, _>>();

    let mut indexed = 0usize;
    for (root_id, records) in grouped {
        let Some(root_path) = root_paths.get(&root_id) else {
            continue;
        };
        indexed += records.len();
        Index::build_from_file_records(
            root_syntext_config(config, repo_root, &root_id, root_path),
            records,
        )
        .map_err(map_index_error)?;
    }
    Ok(indexed)
}

pub(crate) fn search_branch_root_index(
    config: &crate::config::Config,
    repo_root: &Path,
    root: &DiscoveryRoot,
    query: &str,
    options: &SearchOptions,
    limit: Option<usize>,
) -> crate::Result<Option<Vec<RootedSearchMatch>>> {
    if root.kind != DiscoveryRootKind::BranchRef || limit == Some(0) {
        return Ok(None);
    }
    let syntext_config =
        root_syntext_config(config, repo_root, &root.discriminant, &root.absolute_path);
    if !syntext_config.index_dir.join("manifest.json").exists() {
        return Ok(None);
    }
    let (effective_options, matcher) = effective_options(options)?;
    let index = Index::open(syntext_config).map_err(map_index_error)?;
    let mut matches = index.search(query, &effective_options).map_err(|err| {
        crate::Error::Other(anyhow::anyhow!(
            "branch-root search failed for `{query}`: {err}"
        ))
    })?;
    if let Some(matcher) = matcher {
        matches.retain(|m| matcher.is_match(&m.path));
    }
    if let Some(max) = limit {
        matches.truncate(max);
    }
    let meta = RootMetadata::from_discovery_root(root);
    Ok(Some(
        matches
            .into_iter()
            .map(|m| RootedSearchMatch {
                root_id: meta.root_id.clone(),
                is_primary_root: meta.is_primary_root,
                root_kind: meta.root_kind.clone(),
                root_label: meta.root_label.clone(),
                root_ref: meta.root_ref.clone(),
                root_commit: meta.root_commit.clone(),
                editable: meta.editable,
                path: m.path,
                line_number: m.line_number,
                line_content: m.line_content,
                byte_offset: m.byte_offset,
                submatch_start: m.submatch_start,
                submatch_end: m.submatch_end,
            })
            .collect(),
    ))
}

pub(crate) fn branch_index_exists(repo_root: &Path, root_id: &str) -> bool {
    roots_index_dir(repo_root)
        .join(root_id)
        .join("manifest.json")
        .exists()
}

fn effective_options(
    options: &SearchOptions,
) -> crate::Result<(SearchOptions, Option<globset::GlobMatcher>)> {
    let Some(filter) = &options.path_filter else {
        return Ok((options.clone(), None));
    };
    if !contains_glob_chars(filter) {
        return Ok((options.clone(), None));
    }
    let glob = Glob::new(filter).map_err(|err| {
        crate::Error::Other(anyhow::anyhow!(
            "invalid path filter glob `{filter}`: {err}"
        ))
    })?;
    let mut effective = options.clone();
    effective.path_filter = Some(extract_non_glob_prefix(filter));
    Ok((effective, Some(glob.compile_matcher())))
}

fn root_syntext_config(
    config: &crate::config::Config,
    repo_root: &Path,
    root_id: &str,
    root_path: &Path,
) -> SyntextConfig {
    SyntextConfig {
        index_dir: roots_index_dir(repo_root).join(root_id),
        repo_root: root_path.to_path_buf(),
        max_file_size: config.max_file_size_bytes,
        ..SyntextConfig::default()
    }
}

fn roots_index_dir(repo_root: &Path) -> PathBuf {
    crate::config::Config::synrepo_dir(repo_root)
        .join("index")
        .join("roots")
}

fn extract_non_glob_prefix(filter: &str) -> String {
    let mut last_slash = 0;
    for (i, c) in filter.char_indices() {
        if matches!(c, '*' | '?' | '[' | '{') {
            return filter[..last_slash].to_string();
        }
        if c == '/' {
            last_slash = i + 1;
        }
    }
    filter.to_string()
}

fn contains_glob_chars(value: &str) -> bool {
    value.contains('*') || value.contains('?') || value.contains('[') || value.contains('{')
}

fn map_index_error(error: syntext::IndexError) -> crate::Error {
    crate::Error::Other(anyhow::anyhow!("branch-root index error: {error}"))
}
