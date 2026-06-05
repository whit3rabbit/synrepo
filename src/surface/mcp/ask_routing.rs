use std::{collections::HashMap, path::Path};

use syntext::SearchOptions;

use crate::surface::context::{compiler::CompiledContextPlan, ContextTarget};

use super::SynrepoState;

const MAX_QUERIES: usize = 10;
const MAX_SEARCH_ROWS: usize = 20;
const MAX_PROMOTED_FILES: usize = 3;

#[derive(Clone, Debug)]
struct Candidate {
    path: String,
    query_hits: usize,
    primary_hits: usize,
    definition_hits: usize,
    name_hits: usize,
    command_probe_hits: usize,
    dynamic_command_hits: usize,
    best_query_rank: usize,
    first_seen: usize,
}

#[derive(Debug)]
struct QueryProfile {
    primary: String,
    declaration_ident: Option<String>,
    identifiers: Vec<String>,
    command_probe: bool,
}

#[derive(Default)]
struct MatchSignal {
    primary_hits: usize,
    definition_hits: usize,
    name_hits: usize,
    command_probe_hits: usize,
    dynamic_command_hits: usize,
}

pub(super) fn augment_plan_with_search_hits(
    state: &SynrepoState,
    ask: &str,
    plan: &mut CompiledContextPlan,
) {
    let queries = planned_search_queries(ask, plan);
    if queries.is_empty() {
        return;
    }
    let profile = QueryProfile::new(ask);
    let candidates = collect_candidates(state, &queries, &profile);
    let promoted = promoted_paths(candidates);
    if promoted.is_empty() {
        return;
    }
    insert_promoted_targets(plan, promoted);
}

fn planned_search_queries(ask: &str, plan: &CompiledContextPlan) -> Vec<String> {
    let mut out = Vec::new();
    push_unique(&mut out, ask.trim().to_string());
    for target in plan.targets.iter().filter(|target| target.kind == "search") {
        push_unique(&mut out, target.target.clone());
        for query in crate::surface::query_terms::fallback_queries(&target.target) {
            push_unique(&mut out, query);
            if out.len() >= MAX_QUERIES {
                return out;
            }
        }
    }
    out
}

fn collect_candidates(
    state: &SynrepoState,
    queries: &[String],
    profile: &QueryProfile,
) -> Vec<Candidate> {
    let mut candidates: HashMap<String, Candidate> = HashMap::new();
    let mut order = 0usize;
    for (query_rank, query) in queries.iter().enumerate() {
        let options = SearchOptions {
            max_results: Some(MAX_SEARCH_ROWS),
            case_insensitive: true,
            ..SearchOptions::default()
        };
        let Ok(rows) = crate::substrate::search_rooted_with_options(
            &state.config,
            &state.repo_root,
            query,
            &options,
        ) else {
            continue;
        };
        for row in rows {
            if !row.editable {
                continue;
            }
            let path = row.path.to_string_lossy().to_string();
            if is_fixture_path(&path) {
                continue;
            }
            let line = String::from_utf8_lossy(&row.line_content);
            let signal = profile.match_signal(&path, &line);
            candidates
                .entry(path.clone())
                .and_modify(|candidate| {
                    candidate.query_hits += 1;
                    candidate.primary_hits += signal.primary_hits;
                    candidate.definition_hits += signal.definition_hits;
                    candidate.name_hits = candidate.name_hits.max(signal.name_hits);
                    candidate.command_probe_hits =
                        candidate.command_probe_hits.max(signal.command_probe_hits);
                    candidate.dynamic_command_hits = candidate
                        .dynamic_command_hits
                        .max(signal.dynamic_command_hits);
                    candidate.best_query_rank = candidate.best_query_rank.min(query_rank);
                })
                .or_insert_with(|| {
                    order += 1;
                    Candidate {
                        path,
                        query_hits: 1,
                        primary_hits: signal.primary_hits,
                        definition_hits: signal.definition_hits,
                        name_hits: signal.name_hits,
                        command_probe_hits: signal.command_probe_hits,
                        dynamic_command_hits: signal.dynamic_command_hits,
                        best_query_rank: query_rank,
                        first_seen: order,
                    }
                });
        }
    }
    candidates.into_values().collect()
}

fn promoted_paths(mut candidates: Vec<Candidate>) -> Vec<String> {
    let has_live_source = candidates.iter().any(|candidate| {
        source_rank(&candidate.path) >= 2 && !is_archived_openspec_path(&candidate.path)
    });
    if has_live_source {
        candidates.retain(|candidate| !is_archived_openspec_path(&candidate.path));
    }
    candidates.sort_by(|a, b| {
        source_rank(&b.path)
            .cmp(&source_rank(&a.path))
            .then_with(|| b.definition_hits.cmp(&a.definition_hits))
            .then_with(|| primary_hit(b).cmp(&primary_hit(a)))
            .then_with(|| b.name_hits.cmp(&a.name_hits))
            .then_with(|| b.command_probe_hits.cmp(&a.command_probe_hits))
            .then_with(|| b.dynamic_command_hits.cmp(&a.dynamic_command_hits))
            .then_with(|| {
                crate::surface::query_terms::score_path_for_query(&b.path, b.query_hits).cmp(
                    &crate::surface::query_terms::score_path_for_query(&a.path, a.query_hits),
                )
            })
            .then_with(|| b.query_hits.cmp(&a.query_hits))
            .then_with(|| a.best_query_rank.cmp(&b.best_query_rank))
            .then_with(|| a.first_seen.cmp(&b.first_seen))
            .then_with(|| a.path.cmp(&b.path))
    });
    candidates
        .into_iter()
        .take(MAX_PROMOTED_FILES)
        .map(|candidate| candidate.path)
        .collect()
}

fn primary_hit(candidate: &Candidate) -> usize {
    usize::from(candidate.primary_hits > 0)
}

fn insert_promoted_targets(plan: &mut CompiledContextPlan, promoted: Vec<String>) {
    let mut targets = Vec::new();
    for path in promoted {
        if plan
            .targets
            .iter()
            .chain(targets.iter())
            .any(|target| target.target == path)
        {
            continue;
        }
        targets.push(ContextTarget {
            kind: "file".to_string(),
            target: path,
            budget: Some("tiny".to_string()),
        });
    }
    if targets.is_empty() {
        return;
    }
    targets.extend(std::mem::take(&mut plan.targets));
    plan.limit = plan.limit.max(targets.len().min(MAX_PROMOTED_FILES));
    plan.targets = targets;
}

fn source_rank(path: &str) -> i32 {
    let lower = path.to_ascii_lowercase();
    if (lower.starts_with("src/") || lower.contains("/src/")) && !is_test_like_path(&lower) {
        4
    } else if lower.starts_with("src/") || lower.contains("/src/") {
        3
    } else if lower.starts_with("tests/") || lower.contains("/tests/") {
        2
    } else if lower.starts_with("docs/") || lower.starts_with("openspec/") {
        1
    } else {
        0
    }
}

fn is_fixture_path(path: &str) -> bool {
    path.starts_with("benches/tasks/") || path.contains("/benches/tasks/")
}

fn is_archived_openspec_path(path: &str) -> bool {
    path.starts_with("openspec/changes/archive/") || path.contains("/openspec/changes/archive/")
}

fn is_test_like_path(lower: &str) -> bool {
    lower.starts_with("tests/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("_test.")
        || lower.contains(".test.")
        || lower.contains("/fixtures/")
        || lower.contains("/examples/")
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.trim().is_empty() && !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

impl QueryProfile {
    fn new(ask: &str) -> Self {
        let primary = ask.trim().to_ascii_lowercase();
        let declaration_ident = declaration_query_identifier(&primary);
        let mut identifiers = Vec::new();
        for term in crate::surface::query_terms::extract_terms(ask) {
            push_identifier(&mut identifiers, &term.text);
        }
        if let Some(ident) = declaration_ident.as_deref() {
            push_identifier(&mut identifiers, ident);
        }
        Self {
            command_probe: is_command_probe(&primary),
            primary,
            declaration_ident,
            identifiers,
        }
    }

    fn match_signal(&self, path: &str, line: &str) -> MatchSignal {
        let lower_line = line.to_ascii_lowercase();
        let mut signal = MatchSignal {
            primary_hits: self.primary_hit(&lower_line),
            ..MatchSignal::default()
        };
        for ident in &self.identifiers {
            if line_declares_identifier(&lower_line, ident) {
                signal.definition_hits += 1;
            }
        }
        signal.name_hits = self.name_hits(path);
        if self.command_probe && is_command_command_path(path) {
            signal.command_probe_hits = 1;
        }
        if self.command_probe && is_dynamic_command_line(&lower_line) {
            signal.dynamic_command_hits = 1;
        }
        signal
    }

    fn primary_hit(&self, lower_line: &str) -> usize {
        if let Some(ident) = self.declaration_ident.as_deref() {
            usize::from(line_declares_identifier(lower_line, ident))
        } else if !self.primary.is_empty() && lower_line.contains(&self.primary) {
            1
        } else {
            0
        }
    }

    fn name_hits(&self, path: &str) -> usize {
        let lower = path.to_ascii_lowercase();
        let stem = Path::new(path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        self.identifiers
            .iter()
            .map(|ident| {
                if stem == *ident {
                    4
                } else if ident.len() >= 4 && stem.contains(ident) {
                    2
                } else if ident.len() >= 4 && lower.contains(ident) {
                    1
                } else {
                    0
                }
            })
            .max()
            .unwrap_or(0)
    }
}

fn declaration_query_identifier(primary: &str) -> Option<String> {
    let trimmed = primary.trim();
    for prefix in ["pub fn ", "fn "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let ident = rest
                .chars()
                .take_while(|ch| is_identifier_char(*ch))
                .collect::<String>();
            if ident.len() >= 3 {
                return Some(ident);
            }
        }
    }
    None
}

fn push_identifier(values: &mut Vec<String>, raw: &str) {
    let trimmed = raw
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | ':'))
        .to_ascii_lowercase();
    if trimmed.len() < 3 || values.iter().any(|existing| existing == &trimmed) {
        return;
    }
    values.push(trimmed.clone());
    if trimmed.contains("::") {
        for part in trimmed.split("::") {
            push_identifier(values, part);
        }
    }
}

fn line_declares_identifier(lower_line: &str, ident: &str) -> bool {
    for marker in [
        "fn ", "struct ", "enum ", "trait ", "mod ", "type ", "const ", "static ",
    ] {
        let mut rest = lower_line;
        while let Some(offset) = rest.find(marker) {
            let after = &rest[offset + marker.len()..];
            if identifier_at_start(after, ident) {
                return true;
            }
            rest = &after[ident.len().min(after.len())..];
        }
    }
    false
}

fn identifier_at_start(text: &str, ident: &str) -> bool {
    text.starts_with(ident)
        && text
            .chars()
            .nth(ident.chars().count())
            .is_none_or(|ch| !is_identifier_char(ch))
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_command_probe(primary: &str) -> bool {
    primary.contains("command::new")
        || primary.contains("std::process")
        || primary.contains("process::command")
}

fn is_command_command_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("src/bin/cli_support/commands/") && !is_test_like_path(&lower)
}

fn is_dynamic_command_line(lower_line: &str) -> bool {
    let Some(offset) = lower_line.find("command::new(") else {
        return false;
    };
    let after = lower_line[offset + "command::new(".len()..].trim_start();
    !after.starts_with('"') && !after.starts_with("r#\"")
}
