//! Root-aware lexical search wrapper over the primary syntext index plus
//! direct scans of configured non-primary discovery roots.

use std::path::{Path, PathBuf};

use globset::Glob;
use syntext::SearchOptions;

use crate::config::Config;

const PRIMARY_ROOT_ID: &str = "primary";

/// Search result annotated with the discovery root that owns the matched file.
#[derive(Clone, Debug)]
pub struct RootedSearchMatch {
    /// Root discriminator from discovery/graph storage.
    pub root_id: String,
    /// True when `root_id` is the primary checkout.
    pub is_primary_root: bool,
    /// Path relative to the owning root.
    pub path: PathBuf,
    /// 1-based line number of the match.
    pub line_number: u32,
    /// Full bytes of the matching line without the trailing newline.
    pub line_content: Vec<u8>,
    /// Byte offset of the start of the first match within the file.
    pub byte_offset: u64,
    /// Byte offset of the first match within `line_content`.
    pub submatch_start: usize,
    /// Exclusive end byte offset of the first match within `line_content`.
    pub submatch_end: usize,
}

impl RootedSearchMatch {
    fn primary(match_: syntext::SearchMatch) -> Self {
        Self {
            root_id: PRIMARY_ROOT_ID.to_string(),
            is_primary_root: true,
            path: match_.path,
            line_number: match_.line_number,
            line_content: match_.line_content,
            byte_offset: match_.byte_offset,
            submatch_start: match_.submatch_start,
            submatch_end: match_.submatch_end,
        }
    }
}

/// Execute lexical search across the primary syntext index plus non-primary roots.
///
/// syntext reopens result paths relative to a single `repo_root`, so the
/// persisted index is intentionally primary-root-only. Linked worktrees and
/// submodules are scanned directly here to preserve root identity without
/// changing syntext's public API.
pub fn search_rooted_with_options(
    config: &Config,
    repo_root: &Path,
    query: &str,
    options: &SearchOptions,
) -> crate::Result<Vec<RootedSearchMatch>> {
    let limit = options.max_results;
    let mut results =
        crate::substrate::index::search_with_options(config, repo_root, query, options)?
            .into_iter()
            .map(RootedSearchMatch::primary)
            .collect::<Vec<_>>();

    if limit.is_none_or(|max| results.len() < max) {
        let remaining = limit.map(|max| max.saturating_sub(results.len()));
        results.extend(scan_non_primary_roots(
            config, repo_root, query, options, remaining,
        )?);
    }
    if let Some(max) = limit {
        results.truncate(max);
    }
    Ok(results)
}

fn scan_non_primary_roots(
    config: &Config,
    repo_root: &Path,
    query: &str,
    options: &SearchOptions,
    limit: Option<usize>,
) -> crate::Result<Vec<RootedSearchMatch>> {
    let mut out = Vec::new();
    if limit == Some(0) || query.is_empty() {
        return Ok(out);
    }

    let matcher = PathMatcher::new(options.path_filter.as_deref())?;
    let query_cmp = if options.case_insensitive {
        query.to_ascii_lowercase()
    } else {
        query.to_string()
    };

    for file in crate::substrate::discover(repo_root, config)?
        .into_iter()
        .filter(|file| file.root_discriminant != PRIMARY_ROOT_ID)
    {
        if !matches_filters(&file.relative_path, options, &matcher) {
            continue;
        }
        scan_file(
            &file.absolute_path,
            &file.relative_path,
            &file.root_discriminant,
            &query_cmp,
            options.case_insensitive,
            limit,
            &mut out,
        )?;
        if limit.is_some_and(|max| out.len() >= max) {
            break;
        }
    }

    Ok(out)
}

fn scan_file(
    absolute_path: &Path,
    relative_path: &str,
    root_id: &str,
    query: &str,
    case_insensitive: bool,
    limit: Option<usize>,
    out: &mut Vec<RootedSearchMatch>,
) -> crate::Result<()> {
    let bytes = std::fs::read(absolute_path)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut byte_offset = 0u64;
    for (idx, line) in text.lines().enumerate() {
        let haystack = if case_insensitive {
            line.to_ascii_lowercase()
        } else {
            line.to_string()
        };
        if let Some(start) = haystack.find(query) {
            out.push(RootedSearchMatch {
                root_id: root_id.to_string(),
                is_primary_root: false,
                path: PathBuf::from(relative_path),
                line_number: idx.saturating_add(1) as u32,
                line_content: line.as_bytes().to_vec(),
                byte_offset: byte_offset + start as u64,
                submatch_start: start,
                submatch_end: start + query.len(),
            });
            if limit.is_some_and(|max| out.len() >= max) {
                break;
            }
        }
        byte_offset += line.len() as u64 + 1;
    }
    Ok(())
}

fn matches_filters(path: &str, options: &SearchOptions, matcher: &PathMatcher) -> bool {
    if !matcher.matches(path) {
        return false;
    }
    let ext = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    if options.file_type.as_deref().is_some_and(|ty| ext != ty) {
        return false;
    }
    if options.exclude_type.as_deref().is_some_and(|ty| ext == ty) {
        return false;
    }
    true
}

enum PathMatcher {
    Any,
    Prefix(String),
    Glob(globset::GlobMatcher),
}

impl PathMatcher {
    fn new(filter: Option<&str>) -> crate::Result<Self> {
        let Some(filter) = filter else {
            return Ok(Self::Any);
        };
        if contains_glob_chars(filter) {
            return Ok(Self::Glob(
                Glob::new(filter)
                    .map_err(|err| {
                        crate::Error::Other(anyhow::anyhow!(
                            "invalid path filter glob `{filter}`: {err}"
                        ))
                    })?
                    .compile_matcher(),
            ));
        }
        Ok(Self::Prefix(filter.to_string()))
    }

    fn matches(&self, path: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Prefix(prefix) => path.starts_with(prefix),
            Self::Glob(matcher) => matcher.is_match(path),
        }
    }
}

fn contains_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}
