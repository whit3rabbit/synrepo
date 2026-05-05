use std::path::Path;

use synrepo::config::Config;
use synrepo::pipeline::context_metrics;
use synrepo::surface::task_route::{
    classify_task_route_with_config, TaskRoute, SIGNAL_DETERMINISTIC_EDIT_CANDIDATE,
};

pub(crate) fn task_route(
    repo_root: &Path,
    task: &str,
    path: Option<&str>,
    json: bool,
) -> anyhow::Result<()> {
    print!("{}", task_route_output(repo_root, task, path, json)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn task_route_output(
    repo_root: &Path,
    task: &str,
    path: Option<&str>,
    json: bool,
) -> anyhow::Result<String> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let config = Config::load(repo_root).unwrap_or_default();
    let route = classify_task_route_with_config(task, path, &config, &synrepo_dir);
    if synrepo_dir.exists() {
        context_metrics::record_task_route_classification_best_effort(&synrepo_dir, &route);
    }

    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(&route)?));
    }

    Ok(render_text(&route))
}

fn render_text(route: &TaskRoute) -> String {
    let mut out = String::new();
    out.push_str(&format!("intent: {}\n", route.intent));
    out.push_str(&format!("confidence: {:.2}\n", route.confidence));
    out.push_str(&format!("budget_tier: {}\n", route.budget_tier));
    out.push_str(&format!("llm_required: {}\n", route.llm_required));
    out.push_str(&format!("routing_strategy: {}\n", route.routing_strategy));
    if let Some(score) = route.semantic_score {
        out.push_str(&format!("semantic_score: {score:.2}\n"));
    }
    out.push_str(&format!("reason: {}\n", route.reason));
    if let Some(candidate) = &route.edit_candidate {
        out.push_str(&format!(
            "edit_candidate: {} ({})\n",
            candidate.intent, candidate.reason
        ));
    }
    if !route.signals.is_empty() {
        out.push_str("signals:\n");
        for signal in &route.signals {
            if signal == SIGNAL_DETERMINISTIC_EDIT_CANDIDATE {
                if let Some(candidate) = &route.edit_candidate {
                    out.push_str(&format!("  {signal} Intent: {}\n", candidate.intent));
                    continue;
                }
            }
            out.push_str(&format!("  {signal}\n"));
        }
    }
    out.push_str("recommended_tools:\n");
    for tool in &route.recommended_tools {
        out.push_str(&format!("  {tool}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_output_includes_edit_signal_intent() {
        let dir = tempfile::tempdir().unwrap();
        let out = task_route_output(
            dir.path(),
            "convert var to const",
            Some("src/app.ts"),
            false,
        )
        .unwrap();

        assert!(out.contains("[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: var-to-const"));
    }
}
