use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::Config;
use crate::pipeline::context_metrics;
use crate::surface::task_route::classify_task_route;

use super::helpers::render_result;
use super::SynrepoState;

/// Parameters for the `synrepo_task_route` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskRouteParams {
    pub repo_root: Option<PathBuf>,
    /// Plain-language task or hook prompt to classify.
    pub task: String,
    /// Optional file path used only for extension-sensitive confidence.
    #[serde(default)]
    pub path: Option<String>,
}

pub fn handle_task_route(state: &SynrepoState, params: TaskRouteParams) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let route = classify_task_route(&params.task, params.path.as_deref());
        let synrepo_dir = Config::synrepo_dir(&state.repo_root);
        context_metrics::record_task_route_classification_best_effort(&synrepo_dir, &route);
        Ok(serde_json::to_value(route)?)
    })();
    render_result(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_route_params_schema_contains_task_and_path() {
        let schema = schemars::schema_for!(TaskRouteParams);
        let json = serde_json::to_value(schema).unwrap();
        let properties = json
            .pointer("/schema/properties")
            .or_else(|| json.pointer("/properties"))
            .expect("schema properties");

        assert!(properties.get("task").is_some());
        assert!(properties.get("path").is_some());
    }
}
