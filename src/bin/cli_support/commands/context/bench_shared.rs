use std::path::{Path, PathBuf};

use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use synrepo::surface::context::{
    ContextBudget, ContextRecipe, ContextScope, ContextShape, GroundingOptions,
};
use walkdir::WalkDir;

/// Known workflow categories for benchmark fixtures.
pub(crate) const KNOWN_CATEGORIES: &[&str] = &[
    "route_to_edit",
    "symbol_explanation",
    "impact_or_risk",
    "test_surface",
];

const KNOWN_TARGET_KINDS: &[&str] = &["file", "symbol", "test"];

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct BenchTask {
    #[serde(default)]
    pub(crate) name: Option<String>,
    pub(crate) category: String,
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) required_targets: Vec<BenchTarget>,
    #[serde(default)]
    pub(crate) scope: Option<ContextScope>,
    #[serde(default)]
    pub(crate) shape: Option<ContextShape>,
    #[serde(default)]
    pub(crate) ground: Option<GroundingOptions>,
    #[serde(default)]
    pub(crate) budget: Option<ContextBudget>,
    #[serde(default)]
    pub(crate) expected_recipe: Option<ContextRecipe>,
    #[serde(default)]
    pub(crate) allowed_context: Option<Vec<BenchTarget>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct BenchTarget {
    pub(crate) kind: String,
    pub(crate) value: String,
}

pub(crate) fn validate_fixture(fixture: &BenchTask) -> anyhow::Result<()> {
    if fixture.query.trim().is_empty() {
        anyhow::bail!("fixture `query` must be non-empty");
    }
    if fixture.category.trim().is_empty() {
        anyhow::bail!("fixture `category` must be non-empty");
    }
    validate_targets("required_targets", &fixture.required_targets)?;
    if let Some(allowed) = &fixture.allowed_context {
        validate_targets("allowed_context", allowed)?;
    }
    Ok(())
}

fn validate_targets(field: &str, targets: &[BenchTarget]) -> anyhow::Result<()> {
    for (idx, target) in targets.iter().enumerate() {
        if target.value.trim().is_empty() {
            anyhow::bail!("{field}[{idx}]: `value` must be non-empty");
        }
        if !KNOWN_TARGET_KINDS.contains(&target.kind.as_str()) {
            anyhow::bail!(
                "{field}[{idx}]: unknown `kind` `{}` (expected one of {})",
                target.kind,
                KNOWN_TARGET_KINDS.join(", ")
            );
        }
    }
    Ok(())
}

pub(crate) fn classify_targets(
    required: &[BenchTarget],
    returned_paths: &[String],
    returned_symbols: &[String],
) -> (Vec<BenchTarget>, Vec<BenchTarget>) {
    let mut hits = Vec::new();
    let mut misses = Vec::new();
    for target in required {
        if target_satisfied(target, returned_paths, returned_symbols) {
            hits.push(target.clone());
        } else {
            misses.push(target.clone());
        }
    }
    (hits, misses)
}

pub(crate) fn wrong_context_rate(
    allowed: Option<&[BenchTarget]>,
    returned_paths: &[String],
    returned_symbols: &[String],
) -> Option<f64> {
    let allowed = allowed?;
    let returned_count = returned_paths.len() + returned_symbols.len();
    if returned_count == 0 {
        return Some(0.0);
    }
    let wrong_paths = returned_paths
        .iter()
        .filter(|path| {
            !allowed
                .iter()
                .any(|target| target_satisfied(target, std::slice::from_ref(path), &[]))
        })
        .count();
    let wrong_symbols = returned_symbols
        .iter()
        .filter(|symbol| {
            !allowed
                .iter()
                .any(|target| target_satisfied(target, &[], std::slice::from_ref(symbol)))
        })
        .count();
    Some((wrong_paths + wrong_symbols) as f64 / returned_count as f64)
}

pub(crate) fn expand_task_glob(repo_root: &Path, pattern: &str) -> anyhow::Result<Vec<PathBuf>> {
    let pattern_abs = repo_root.join(pattern).to_string_lossy().to_string();
    let glob = Glob::new(&pattern_abs)?;
    let mut builder = GlobSetBuilder::new();
    builder.add(glob);
    let set = builder.build()?;
    let walk_root = fixed_prefix(&pattern_abs).unwrap_or_else(|| repo_root.to_path_buf());
    let mut paths = Vec::new();
    for entry in WalkDir::new(&walk_root).into_iter().filter_map(Result::ok) {
        if entry.file_type().is_file() && set.is_match(entry.path()) {
            paths.push(entry.path().to_path_buf());
        }
    }
    paths.sort();
    Ok(paths)
}

fn target_satisfied(
    target: &BenchTarget,
    returned_paths: &[String],
    returned_symbols: &[String],
) -> bool {
    match target.kind.as_str() {
        "symbol" => {
            returned_symbols
                .iter()
                .any(|got| got.contains(&target.value))
                || path_contains(returned_paths, &target.value)
        }
        _ => path_contains(returned_paths, &target.value),
    }
}

fn path_contains(returned_paths: &[String], needle: &str) -> bool {
    returned_paths.iter().any(|got| got.contains(needle))
}

fn fixed_prefix(pattern: &str) -> Option<PathBuf> {
    let cutoff = pattern.find(['*', '?', '[', '{']).unwrap_or(pattern.len());
    let head = &pattern[..cutoff];
    head.rfind('/').map(|idx| PathBuf::from(&head[..idx]))
}
