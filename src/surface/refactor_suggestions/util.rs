use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use globset::Glob;

use crate::structure::graph::{SymbolNode, Visibility};
use crate::substrate::DiscoveryRoot;

use super::{RefactorSuggestionCandidate, RefactorSuggestionGroup, RefactorSymbolCounts};

pub(crate) fn root_path_map(roots: &[DiscoveryRoot]) -> BTreeMap<String, PathBuf> {
    roots
        .iter()
        .map(|root| (root.discriminant.clone(), root.absolute_path.clone()))
        .collect()
}

pub(crate) fn physical_line_count(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    bytes.iter().filter(|byte| **byte == b'\n').count() + usize::from(!bytes.ends_with(b"\n"))
}

pub(crate) fn symbol_counts(symbols: &[SymbolNode]) -> RefactorSymbolCounts {
    let mut counts = RefactorSymbolCounts {
        total: symbols.len(),
        public: 0,
        restricted: 0,
        private: 0,
    };
    for sym in symbols {
        match sym.visibility {
            Visibility::Public => counts.public += 1,
            Visibility::Crate | Visibility::Protected => counts.restricted += 1,
            Visibility::Private => counts.private += 1,
            Visibility::Unknown => {}
        }
    }
    counts
}

pub(crate) fn groups_for(
    candidates: &[RefactorSuggestionCandidate],
) -> Vec<RefactorSuggestionGroup> {
    let mut groups: BTreeMap<String, RefactorSuggestionGroup> = BTreeMap::new();
    for candidate in candidates {
        let language = candidate
            .language
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let group = groups
            .entry(language.clone())
            .or_insert(RefactorSuggestionGroup {
                language,
                count: 0,
                max_line_count: 0,
                max_missing_public_doc_count: 0,
            });
        group.count += 1;
        group.max_line_count = group.max_line_count.max(candidate.line_count);
        group.max_missing_public_doc_count = group
            .max_missing_public_doc_count
            .max(candidate.missing_public_doc_count);
    }
    groups.into_values().collect()
}

pub(crate) fn is_test_file_path(path: &str) -> bool {
    let Some(name) = file_name(path) else {
        return false;
    };
    path.split('/')
        .any(|part| matches!(part, "tests" | "__tests__"))
        || name == "tests.rs"
        || name.starts_with("test_")
        || name.contains("_test.")
        || name.contains("_tests.")
        || name.contains(".test.")
        || name.contains(".spec.")
}

pub(crate) fn file_name(path: &str) -> Option<&str> {
    Path::new(path).file_name().and_then(|name| name.to_str())
}

pub(crate) fn file_stem(path: &str) -> Option<&str> {
    Path::new(path).file_stem().and_then(|stem| stem.to_str())
}

pub(crate) struct PathMatcher {
    filter: Option<PathFilter>,
}

enum PathFilter {
    Prefix(String),
    Glob(globset::GlobMatcher),
}

impl PathMatcher {
    pub(crate) fn new(filter: Option<&str>) -> crate::Result<Self> {
        let Some(filter) = filter.filter(|value| !value.trim().is_empty()) else {
            return Ok(Self { filter: None });
        };
        if contains_glob_chars(filter) {
            let glob = Glob::new(filter).map_err(|err| {
                crate::Error::Config(format!("invalid path filter glob `{filter}`: {err}"))
            })?;
            Ok(Self {
                filter: Some(PathFilter::Glob(glob.compile_matcher())),
            })
        } else {
            Ok(Self {
                filter: Some(PathFilter::Prefix(filter.to_string())),
            })
        }
    }

    pub(crate) fn matches(&self, path: &str) -> bool {
        match &self.filter {
            None => true,
            Some(PathFilter::Prefix(prefix)) => path.starts_with(prefix),
            Some(PathFilter::Glob(glob)) => glob.is_match(path),
        }
    }
}

fn contains_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}
