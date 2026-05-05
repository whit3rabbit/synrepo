use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use synrepo::registry;
use synrepo::surface::mcp::SynrepoState;

use super::SynrepoServer;

mod session;
pub(crate) use session::SessionState;

const OVERLAY_WRITE_TOOLS: &[&str] = &[
    "synrepo_refresh_commentary",
    "synrepo_note_add",
    "synrepo_note_link",
    "synrepo_note_supersede",
    "synrepo_note_forget",
    "synrepo_note_verify",
];

const SOURCE_EDIT_TOOLS: &[&str] = &["synrepo_prepare_edit_context", "synrepo_apply_anchor_edits"];

#[derive(Clone)]
pub(crate) struct StateResolver {
    states: Arc<RwLock<HashMap<PathBuf, Arc<SynrepoState>>>>,
    // Per-root prepare lock so a slow cold-load for one repo does not block
    // unrelated repos. Held only across `prepare_state`; the global `states`
    // RwLock is taken briefly for read and write.
    prepare_locks: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>,
    default_repo_root: Arc<RwLock<Option<PathBuf>>>,
    default_requires_registry: Arc<RwLock<bool>>,
}

impl StateResolver {
    pub(crate) fn new(default_state: Option<SynrepoState>) -> Self {
        let mut states = HashMap::new();
        let default_repo_root = default_state
            .as_ref()
            .map(|state| registry::canonicalize_path(&state.repo_root));
        if let Some(state) = default_state {
            let key = default_repo_root
                .clone()
                .expect("default key exists when default state exists");
            states.insert(key, Arc::new(state));
        }
        Self {
            states: Arc::new(RwLock::new(states)),
            prepare_locks: Arc::new(Mutex::new(HashMap::new())),
            default_repo_root: Arc::new(RwLock::new(default_repo_root)),
            default_requires_registry: Arc::new(RwLock::new(false)),
        }
    }

    pub(crate) fn resolve(&self, param_root: Option<PathBuf>) -> anyhow::Result<Arc<SynrepoState>> {
        let root = match param_root {
            Some(root) => registry::canonicalize_path(&root),
            None => self.default_repo_root.read().clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "repo_root is required for this synrepo MCP server. \
                     Pass the absolute workspace path as repo_root, or run \
                     `synrepo project add <path>` before using a global MCP integration."
                )
            })?,
        };

        if self.requires_registry(&root) {
            require_registered_project(&root)?;
        }

        if let Some(state) = self.states.read().get(&root).cloned() {
            return Ok(state);
        }

        self.cached_or_prepare(root)
    }

    pub(crate) fn set_default(&self, root: PathBuf) -> anyhow::Result<Arc<SynrepoState>> {
        let root = registry::canonicalize_path(&root);
        require_registered_project(&root)?;
        let state = self.cached_or_prepare(root.clone())?;
        *self.default_repo_root.write() = Some(root);
        *self.default_requires_registry.write() = true;
        Ok(state)
    }

    fn requires_registry(&self, root: &Path) -> bool {
        if self.is_default_root(root) {
            return *self.default_requires_registry.read();
        }
        true
    }

    fn is_default_root(&self, root: &Path) -> bool {
        self.default_repo_root
            .read()
            .as_deref()
            .map(|default| default == root)
            .unwrap_or(false)
    }

    fn cached_or_prepare(&self, root: PathBuf) -> anyhow::Result<Arc<SynrepoState>> {
        if let Some(state) = self.states.read().get(&root).cloned() {
            return Ok(state);
        }

        let prepare_lock = {
            let mut locks = self.prepare_locks.lock();
            Arc::clone(
                locks
                    .entry(root.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(()))),
            )
        };
        let _guard = prepare_lock.lock();

        if let Some(state) = self.states.read().get(&root).cloned() {
            return Ok(state);
        }

        let state = super::super::mcp_runtime::prepare_state(&root)
            .map_err(|error| anyhow::anyhow!("failed to prepare {}: {error:#}", root.display()))?;
        let state = Arc::new(state);
        self.states.write().insert(root, Arc::clone(&state));
        Ok(state)
    }
}

pub(crate) fn render_state_error(error: anyhow::Error) -> String {
    synrepo::surface::mcp::error::error_json(error)
}
fn require_registered_project(root: &Path) -> anyhow::Result<()> {
    if registry::contains_project(root)? {
        return Ok(());
    }
    Err(synrepo::surface::mcp::error::McpError::not_found(format!(
        "repository is not managed by synrepo: {}. Run `synrepo project add {}` to register it.",
        root.display(),
        root.display()
    ))
    .into())
}

impl Clone for SynrepoServer {
    fn clone(&self) -> Self {
        Self {
            resolver: self.resolver.clone(),
            tool_router: self.tool_router.clone(),
            allow_overlay_writes: self.allow_overlay_writes,
            allow_source_edits: self.allow_source_edits,
            session: self.session.clone(),
            call_timeout: self.call_timeout,
        }
    }
}

impl SynrepoServer {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new(state: SynrepoState, allow_source_edits: bool) -> Self {
        Self::new_optional(Some(state), allow_source_edits)
    }

    pub(crate) fn new_optional(
        default_state: Option<SynrepoState>,
        allow_source_edits: bool,
    ) -> Self {
        Self::new_optional_with_overlay(default_state, false, allow_source_edits)
    }

    pub(crate) fn new_optional_with_overlay(
        default_state: Option<SynrepoState>,
        allow_overlay_writes: bool,
        allow_source_edits: bool,
    ) -> Self {
        Self::new_optional_with_timeout(
            default_state,
            allow_overlay_writes,
            allow_source_edits,
            std::time::Duration::from_secs(30),
        )
    }

    pub(crate) fn new_optional_with_timeout(
        default_state: Option<SynrepoState>,
        allow_overlay_writes: bool,
        allow_source_edits: bool,
        call_timeout: std::time::Duration,
    ) -> Self {
        let resolver = StateResolver::new(default_state);
        let mut tool_router = Self::build_tool_router();
        if !allow_overlay_writes {
            for tool in OVERLAY_WRITE_TOOLS {
                tool_router.remove_route(tool);
            }
        }
        if !allow_source_edits {
            for tool in SOURCE_EDIT_TOOLS {
                tool_router.remove_route(tool);
            }
        }
        Self {
            resolver,
            tool_router,
            allow_overlay_writes,
            allow_source_edits,
            session: SessionState::default(),
            call_timeout,
        }
    }

    pub(super) fn resolve_state(
        &self,
        param_root: Option<PathBuf>,
    ) -> anyhow::Result<Arc<SynrepoState>> {
        self.resolver.resolve(param_root)
    }

    pub(super) fn with_tool_state<F>(
        &self,
        tool: &'static str,
        param_root: Option<PathBuf>,
        f: F,
    ) -> String
    where
        F: FnOnce(Arc<SynrepoState>) -> String,
    {
        if let Err(error) = self.session.check_rate_limit(tool) {
            return render_state_error(error);
        }
        match self.resolve_state(param_root) {
            Ok(state) => {
                let synrepo_dir = synrepo::config::Config::synrepo_dir(&state.repo_root);
                let output =
                    synrepo::pipeline::explain::telemetry::with_synrepo_dir(&synrepo_dir, || {
                        f(Arc::clone(&state))
                    });
                let output = synrepo::surface::mcp::response_budget::clamp_and_record_response(
                    &synrepo_dir,
                    tool,
                    output,
                );
                let errored = response_has_error(&output);
                self.session.record_tool(tool, errored);
                let saved_context = saved_context_metric(tool, errored);
                self.record_tool_result_for(&state, tool, errored, saved_context);
                output
            }
            Err(error) => {
                self.session.record_tool(tool, true);
                render_state_error(error)
            }
        }
    }

    pub(super) async fn with_tool_state_blocking<F>(
        &self,
        tool: &'static str,
        param_root: Option<PathBuf>,
        f: F,
    ) -> String
    where
        F: FnOnce(Arc<SynrepoState>) -> String + Send + 'static,
    {
        let server = self.clone();
        let task = tokio::task::spawn_blocking(move || server.with_tool_state(tool, param_root, f));
        match tokio::time::timeout(self.call_timeout, task).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => render_state_error(anyhow::anyhow!("MCP tool task failed: {error}")),
            Err(_) => render_state_error(
                synrepo::surface::mcp::error::McpError::timeout(format!(
                    "MCP tool {tool} exceeded {}s timeout",
                    self.call_timeout.as_secs()
                ))
                .into(),
            ),
        }
    }

    pub(super) async fn with_tool_state_persistent<F>(
        &self,
        tool: &'static str,
        param_root: Option<PathBuf>,
        f: F,
    ) -> String
    where
        F: FnOnce(Arc<SynrepoState>) -> String + Send + 'static,
    {
        let server = self.clone();
        let task = tokio::task::spawn_blocking(move || server.with_tool_state(tool, param_root, f));
        match task.await {
            Ok(output) => output,
            Err(error) => render_state_error(anyhow::anyhow!("MCP tool task failed: {error}")),
        }
    }

    pub(super) fn use_project(&self, repo_root: PathBuf) -> String {
        let output = match self.resolver.set_default(repo_root) {
            Ok(state) => serde_json::json!({
                "status": "default_set",
                "repo_root": state.repo_root,
            })
            .to_string(),
            Err(error) => render_state_error(error),
        };
        let errored = response_has_error(&output);
        self.session.record_tool("synrepo_use_project", errored);
        output
    }

    pub(super) fn metrics_for_repo_root(&self, repo_root: Option<PathBuf>) -> String {
        let state = match repo_root {
            Some(repo_root) => match self.resolve_state(Some(repo_root)) {
                Ok(state) => Some(state),
                Err(error) => {
                    self.session.record_tool("synrepo_metrics", true);
                    return render_state_error(error);
                }
            },
            None => self.resolve_state(None).ok(),
        };
        self.metrics_json(state.as_deref())
    }

    pub(super) fn metrics_json(&self, state: Option<&SynrepoState>) -> String {
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
        let errored = response_has_error(&output);
        self.session.record_tool("synrepo_metrics", errored);
        output
    }

    pub(super) fn record_tool_result_for(
        &self,
        state: &SynrepoState,
        tool: &str,
        errored: bool,
        saved_context_write: Option<&str>,
    ) {
        let synrepo_dir = synrepo::config::Config::synrepo_dir(&state.repo_root);
        synrepo::pipeline::context_metrics::record_mcp_tool_result_best_effort(
            &synrepo_dir,
            tool,
            errored,
            saved_context_write,
        );
    }

    pub(super) fn record_resource_for(&self, state: &SynrepoState) {
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

fn response_has_error(output: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(output)
        .ok()
        .and_then(|value| value.get("ok").and_then(|ok| ok.as_bool()).map(|ok| !ok))
        .unwrap_or(false)
}

fn saved_context_metric(tool: &str, errored: bool) -> Option<&'static str> {
    if errored {
        return None;
    }
    match tool {
        "synrepo_note_add" => Some("note_add"),
        "synrepo_note_link" => Some("note_link"),
        "synrepo_note_supersede" => Some("note_supersede"),
        "synrepo_note_forget" => Some("note_forget"),
        "synrepo_note_verify" => Some("note_verify"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "state_registry_tests.rs"]
mod registry_tests;
#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
