use std::time::Instant;

use serde_json::json;

use super::{
    card_render::render_card_target,
    cards::CardParams,
    helpers::{parse_budget, render_result},
    limits::{DEFAULT_RESPONSE_TOKEN_CAP, MAX_CARD_TARGETS, MAX_DEEP_CARD_TARGETS},
    response_budget::estimate_json_tokens,
    SynrepoState,
};

pub fn handle_degraded_card(
    repo_root: Option<std::path::PathBuf>,
    params: CardParams,
    error: anyhow::Error,
) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let target = params
            .target
            .or_else(|| params.targets.into_iter().next())
            .ok_or_else(|| super::error::McpError::invalid_parameter("target is required"))?;
        let Some(repo_root) = repo_root else {
            return Err(error);
        };
        let abs = repo_root.join(&target);
        let git_status = std::process::Command::new("git")
            .args(["status", "--porcelain", "--", &target])
            .current_dir(&repo_root)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());
        let path = target.clone();
        Ok(json!({
            "source_store": "degraded",
            "target": target,
            "path": path,
            "repo_root": repo_root,
            "exists": abs.exists(),
            "is_file": abs.is_file(),
            "git_status": git_status,
            "error": super::error::error_value(&error)["error"].clone(),
            "context_accounting": {
                "budget_tier": params.budget,
                "token_estimate": 0,
                "raw_file_token_estimate": 0,
                "estimated_savings_ratio": 0.0,
                "source_hashes": [],
                "stale": true,
                "truncation_applied": false
            }
        }))
    })();
    render_result(result)
}

pub fn handle_card_params(state: &SynrepoState, params: CardParams) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let mut targets = params.targets;
        if let Some(target) = params.target {
            if targets.is_empty() {
                targets.push(target);
            } else {
                targets.insert(0, target);
            }
        }
        if targets.is_empty() {
            return Err(
                super::error::McpError::invalid_parameter("target or targets is required").into(),
            );
        }
        if targets.len() == 1 {
            let budget = parse_budget(&params.budget)?;
            return state
                .with_read_compiler(|compiler| {
                    render_card_target(
                        state,
                        compiler,
                        &targets[0],
                        budget,
                        params.budget_tokens,
                        params.include_notes,
                        Instant::now(),
                    )
                    .map_err(crate::Error::from)
                })
                .map_err(|err| anyhow::anyhow!(err));
        }
        if targets.len() > MAX_CARD_TARGETS {
            return Err(super::error::McpError::invalid_parameter(format!(
                "targets has {} entries, exceeding limit {MAX_CARD_TARGETS}",
                targets.len()
            ))
            .into());
        }
        let budget = parse_budget(&params.budget)?;
        if budget == crate::surface::card::Budget::Deep && targets.len() > MAX_DEEP_CARD_TARGETS {
            return Err(super::error::McpError::invalid_parameter(format!(
                "deep card batches are capped at {MAX_DEEP_CARD_TARGETS} targets; use tiny/normal or narrow the request"
            ))
            .into());
        }
        let mut rendered = Vec::new();
        let mut errors = Vec::new();
        state
            .with_read_compiler(|compiler| {
                for target in targets {
                    match render_card_target(
                        state,
                        compiler,
                        &target,
                        budget,
                        params.budget_tokens,
                        params.include_notes,
                        Instant::now(),
                    ) {
                        Ok(card) => rendered.push((target, card)),
                        Err(error) => errors.push(json!({
                            "target": target,
                            "error": super::error::error_value(&error),
                        })),
                    }
                }
                Ok(())
            })
            .map_err(|err| anyhow::anyhow!(err))?;
        let original_count = rendered.len();
        let batch_budget_tokens = params.budget_tokens.or(Some(DEFAULT_RESPONSE_TOKEN_CAP));
        let (cards, omitted, batch_truncated) = apply_batch_cap(rendered, batch_budget_tokens);
        let card_count = cards.len();
        let batch_token_estimate = estimate_json_tokens(&json!({ "cards": &cards }));
        Ok(json!({
            "budget": params.budget,
            "cards": cards,
            "errors": errors,
            "total": card_count,
            "omitted": omitted,
            "context_accounting": {
                "budget_tier": params.budget,
                "token_estimate": batch_token_estimate,
                "raw_file_token_estimate": 0,
                "estimated_savings_ratio": 0.0,
                "source_hashes": [],
                "stale": false,
                "truncation_applied": batch_truncated,
                "original_card_count": original_count
            }
        }))
    })();
    render_result(result)
}

fn apply_batch_cap(
    rendered: Vec<(String, serde_json::Value)>,
    budget_tokens: Option<usize>,
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>, bool) {
    let Some(cap) = budget_tokens else {
        return (
            rendered.into_iter().map(|(_, card)| card).collect(),
            Vec::new(),
            false,
        );
    };
    let mut used = 0usize;
    let mut cards = Vec::new();
    let mut omitted = Vec::new();
    let mut truncated = false;
    for (idx, (target, mut card)) in rendered.into_iter().enumerate() {
        let tokens = estimate_json_tokens(&card);
        if used + tokens > cap && idx > 0 {
            omitted.push(json!({
                "target": target,
                "reason": "budget_tokens_exceeded",
            }));
            truncated = true;
            continue;
        }
        if used + tokens > cap {
            mark_card_truncated(&mut card);
            truncated = true;
        }
        used += tokens;
        cards.push(card);
    }
    (cards, omitted, truncated)
}

fn mark_card_truncated(card: &mut serde_json::Value) {
    if let Some(obj) = card
        .get_mut("context_accounting")
        .and_then(|value| value.as_object_mut())
    {
        obj.insert(
            "truncation_applied".to_string(),
            serde_json::Value::Bool(true),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::{bootstrap::bootstrap, config::Config};

    fn make_state() -> (tempfile::TempDir, SynrepoState) {
        let home = tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let dir = tempdir().unwrap();
        let repo = dir.path();
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::create_dir_all(repo.join("notes")).unwrap();
        for idx in 0..4 {
            fs::write(
                repo.join(format!("src/file{idx}.rs")),
                format!("pub fn unique_card_batch_{idx}() {{}}\n"),
            )
            .unwrap();
        }
        for idx in 0..10 {
            fs::write(
                repo.join(format!("notes/file{idx}.txt")),
                format!("# Note {idx}\n{}\n", "alpha ".repeat(1000)),
            )
            .unwrap();
        }
        bootstrap(repo, None, false).unwrap();
        let state = SynrepoState {
            config: Config::load(repo).unwrap(),
            repo_root: repo.to_path_buf(),
        };
        (dir, state)
    }

    fn params(targets: Vec<&str>, budget: &str, budget_tokens: Option<usize>) -> CardParams {
        CardParams {
            repo_root: None,
            target: None,
            targets: targets.into_iter().map(str::to_string).collect(),
            budget: budget.to_string(),
            budget_tokens,
            include_notes: false,
        }
    }

    #[test]
    fn deep_card_batch_rejects_more_than_three_targets() {
        let (_dir, state) = make_state();
        let output = handle_card_params(
            &state,
            params(
                vec![
                    "src/file0.rs",
                    "src/file1.rs",
                    "src/file2.rs",
                    "src/file3.rs",
                ],
                "deep",
                None,
            ),
        );
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "INVALID_PARAMETER");
    }

    #[test]
    fn card_batch_reports_omitted_targets_over_budget() {
        let (_dir, state) = make_state();
        let output = handle_card_params(
            &state,
            params(vec!["src/file0.rs", "src/file1.rs"], "tiny", Some(1)),
        );
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(value["cards"].as_array().unwrap().len(), 1);
        assert_eq!(value["omitted"][0]["target"], "src/file1.rs");
        assert_eq!(value["context_accounting"]["truncation_applied"], true);
    }

    #[test]
    fn directory_target_routes_to_module_card() {
        let (_dir, state) = make_state();
        let mut request = params(Vec::new(), "tiny", None);
        request.target = Some("src".to_string());
        let value: serde_json::Value =
            serde_json::from_str(&handle_card_params(&state, request)).unwrap();

        assert_eq!(value["path"], "src/");
        assert_eq!(value["source_store"], "graph");
        assert!(value["files"]
            .as_array()
            .is_some_and(|files| files.len() == 4));
    }

    #[test]
    fn existing_non_graph_text_file_returns_filesystem_fallback() {
        let (_dir, state) = make_state();
        let mut request = params(Vec::new(), "tiny", None);
        request.target = Some("notes/file0.txt".to_string());
        let value: serde_json::Value =
            serde_json::from_str(&handle_card_params(&state, request)).unwrap();

        assert_eq!(value["card_type"], "filesystem_fallback");
        assert_eq!(value["graph_backed"], false);
        assert_eq!(value["path"], "notes/file0.txt");
        assert!(value["headings"]
            .as_array()
            .is_some_and(|headings| headings.iter().any(|heading| heading == "Note 0")));
    }

    #[test]
    fn card_batch_defaults_to_internal_cap_and_omits_targets() {
        let (_dir, state) = make_state();
        let targets = (0..10)
            .map(|idx| format!("notes/file{idx}.txt"))
            .collect::<Vec<_>>();
        let request = CardParams {
            repo_root: None,
            target: None,
            targets,
            budget: "normal".to_string(),
            budget_tokens: None,
            include_notes: false,
        };
        let value: serde_json::Value =
            serde_json::from_str(&handle_card_params(&state, request)).unwrap();

        assert!(value["cards"].as_array().unwrap().len() < 10, "{value}");
        assert!(value["omitted"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
        assert_eq!(value["context_accounting"]["truncation_applied"], true);
    }
}
