use std::path::PathBuf;

use synrepo::surface::mcp::SynrepoState;

use super::{outcome::response_error_code, render_state_error};
use crate::cli_support::commands::mcp::{sentry_telemetry, SynrepoServer};

impl SynrepoServer {
    pub(crate) fn use_project(&self, repo_root: PathBuf) -> String {
        let output = match self.resolver.set_default(repo_root) {
            Ok(state) => serde_json::json!({
                "status": "default_set",
                "repo_root": state.repo_root,
            })
            .to_string(),
            Err(error) => render_state_error(error),
        };
        let error_code = response_error_code(&output);
        let errored = error_code.is_some();
        self.session.record_tool("synrepo_use_project", errored);
        if let Some(code) = error_code.as_deref() {
            sentry_telemetry::capture_failed_tool_call("synrepo_use_project", code);
        }
        output
    }

    pub(crate) fn metrics_for_repo_root(&self, repo_root: Option<PathBuf>) -> String {
        let state = match repo_root {
            Some(repo_root) => match self.resolve_state(Some(repo_root)) {
                Ok(state) => Some(state),
                Err(error) => {
                    let code = synrepo::surface::mcp::error::classify_error(&error).as_str();
                    sentry_telemetry::capture_failed_tool_call("synrepo_metrics", code);
                    self.session.record_tool("synrepo_metrics", true);
                    return render_state_error(error);
                }
            },
            None => self.resolve_state(None).ok(),
        };
        self.metrics_json(state.as_deref())
    }

    pub(crate) fn metrics_json(&self, state: Option<&SynrepoState>) -> String {
        let persisted = state.and_then(|state| {
            let synrepo_dir = synrepo::config::Config::synrepo_dir(&state.repo_root);
            synrepo::pipeline::context_metrics::load_optional(&synrepo_dir)
                .ok()
                .flatten()
        });
        let output = serde_json::to_string_pretty(&serde_json::json!({
            "this_session": self.session.snapshot(),
            "persisted": persisted,
        }))
        .unwrap_or_else(|err| render_state_error(anyhow::anyhow!(err)));
        let error_code = response_error_code(&output);
        let errored = error_code.is_some();
        self.session.record_tool("synrepo_metrics", errored);
        if let Some(code) = error_code.as_deref() {
            sentry_telemetry::capture_failed_tool_call("synrepo_metrics", code);
        }
        output
    }

    pub(crate) fn record_tool_result_for(
        &self,
        state: &SynrepoState,
        tool: &str,
        error_code: Option<&str>,
        saved_context_write: Option<&str>,
    ) {
        let synrepo_dir = synrepo::config::Config::synrepo_dir(&state.repo_root);
        synrepo::pipeline::context_metrics::record_mcp_tool_result_best_effort(
            &synrepo_dir,
            tool,
            error_code,
            saved_context_write,
        );
    }

    pub(crate) fn record_resource_for(&self, state: &SynrepoState) {
        let synrepo_dir = synrepo::config::Config::synrepo_dir(&state.repo_root);
        synrepo::pipeline::context_metrics::record_mcp_resource_read_best_effort(&synrepo_dir);
    }

    #[cfg(test)]
    pub(crate) fn registered_tool_names(&self) -> Vec<String> {
        self.tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect()
    }
}
