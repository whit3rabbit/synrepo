use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde_json::json;
use synrepo::registry;
use synrepo::surface::mcp::SynrepoState;

use super::SynrepoServer;

#[derive(Clone)]
pub(crate) struct StateResolver {
    states: Arc<RwLock<HashMap<PathBuf, Arc<SynrepoState>>>>,
    // Per-root prepare lock so a slow cold-load for one repo does not block
    // unrelated repos. Held only across `prepare_state`; the global `states`
    // RwLock is taken briefly for read and write.
    prepare_locks: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>,
    default_repo_root: Option<PathBuf>,
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
            default_repo_root,
        }
    }

    pub(crate) fn resolve(&self, param_root: Option<PathBuf>) -> anyhow::Result<Arc<SynrepoState>> {
        let root = match param_root {
            Some(root) => registry::canonicalize_path(&root),
            None => self.default_repo_root.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "repo_root is required for this synrepo MCP server. \
                     Pass the absolute workspace path as repo_root, or run \
                     `synrepo project add <path>` before using a global MCP integration."
                )
            })?,
        };

        // A cached state implies this root was validated on the first call;
        // skip the per-request registry read for the steady-state hot path.
        if let Some(state) = self.states.read().get(&root).cloned() {
            return Ok(state);
        }

        if !self.is_default_root(&root) {
            require_registered_project(&root)?;
        }

        self.cached_or_prepare(root)
    }

    fn is_default_root(&self, root: &Path) -> bool {
        self.default_repo_root
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
    serde_json::to_string_pretty(&json!({ "error": error.to_string() }))
        .unwrap_or_else(|_| r#"{"error":"serialization failure"}"#.to_string())
}

fn require_registered_project(root: &Path) -> anyhow::Result<()> {
    if registry::contains_project(root)? {
        return Ok(());
    }
    anyhow::bail!(
        "repository is not managed by synrepo: {}. Run `synrepo project add {}` to register it.",
        root.display(),
        root.display()
    )
}

impl Clone for SynrepoServer {
    fn clone(&self) -> Self {
        Self {
            resolver: self.resolver.clone(),
            tool_router: self.tool_router.clone(),
            allow_edits: self.allow_edits,
        }
    }
}

impl SynrepoServer {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new(state: SynrepoState, allow_edits: bool) -> Self {
        Self::new_optional(Some(state), allow_edits)
    }

    pub(crate) fn new_optional(default_state: Option<SynrepoState>, allow_edits: bool) -> Self {
        let resolver = StateResolver::new(default_state);
        let mut tool_router = Self::build_tool_router();
        if !allow_edits {
            tool_router.remove_route("synrepo_prepare_edit_context");
            tool_router.remove_route("synrepo_apply_anchor_edits");
        }
        Self {
            resolver,
            tool_router,
            allow_edits,
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
        match self.resolve_state(param_root) {
            Ok(state) => {
                let output = f(Arc::clone(&state));
                let errored = response_has_error(&output);
                let saved_context = saved_context_metric(tool, errored);
                self.record_tool_result_for(&state, tool, errored, saved_context);
                output
            }
            Err(error) => render_state_error(error),
        }
    }

    /// Best-effort recording of a workflow alias call. Keeps
    /// `workflow_calls_total` in the context-metrics file separate from
    /// card-level counters so the two categories never collapse into a
    /// single aggregate.
    pub(super) fn record_workflow_for(&self, state: &SynrepoState, tool: &str) {
        let synrepo_dir = synrepo::config::Config::synrepo_dir(&state.repo_root);
        synrepo::pipeline::context_metrics::record_workflow_call_best_effort(&synrepo_dir, tool);
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

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn stop_auto_started_watchers(&self) {}
}

fn response_has_error(output: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(output)
        .ok()
        .and_then(|value| value.get("error").cloned())
        .is_some()
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
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::{tempdir, TempDir};

    use super::*;
    use crate::cli_support::commands::mcp_runtime::prepare_state;
    use synrepo::bootstrap::bootstrap;
    use synrepo::config::{test_home, Config};
    use synrepo::pipeline::watch::{watch_service_status, WatchServiceStatus};
    use synrepo::store::sqlite::SqliteGraphStore;

    struct HomeFixture {
        _lock: synrepo::test_support::GlobalTestLock,
        _home: TempDir,
        _guard: test_home::HomeEnvGuard,
    }

    fn home_fixture() -> HomeFixture {
        let lock = synrepo::test_support::global_test_lock(test_home::HOME_ENV_TEST_LOCK);
        let home = tempdir().unwrap();
        let guard = test_home::HomeEnvGuard::redirect_to(home.path());
        HomeFixture {
            _lock: lock,
            _home: home,
            _guard: guard,
        }
    }

    fn ready_repo(body: &str) -> (TempDir, PathBuf) {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/lib.rs"), body).unwrap();
        bootstrap(dir.path(), None, false).unwrap();
        let path = dir.path().to_path_buf();
        (dir, path)
    }

    fn resolve_error(result: anyhow::Result<Arc<SynrepoState>>) -> anyhow::Error {
        match result {
            Ok(_) => panic!("state resolution unexpectedly succeeded"),
            Err(error) => error,
        }
    }

    #[test]
    fn default_repo_resolution_uses_prepared_default() {
        let _home = home_fixture();
        let (_repo, repo_path) = ready_repo("pub fn default_repo_needle() {}\n");
        let state = prepare_state(&repo_path).unwrap();
        let resolver = StateResolver::new(Some(state));

        let resolved = resolver.resolve(None).unwrap();
        assert_eq!(resolved.repo_root, repo_path);
    }

    #[test]
    fn defaultless_missing_repo_root_is_loud() {
        let _home = home_fixture();
        let resolver = StateResolver::new(None);

        let err = resolve_error(resolver.resolve(None)).to_string();
        assert!(err.contains("repo_root is required"), "{err}");
    }

    #[test]
    fn registered_repo_root_lazy_loads_and_routes_handlers() {
        let _home = home_fixture();
        let (_default_repo, default_path) = ready_repo("pub fn default_needle() {}\n");
        let (_target_repo, target_path) = ready_repo("pub fn target_needle_unique() {}\n");
        registry::record_project(&target_path).unwrap();
        let default_state = prepare_state(&default_path).unwrap();
        let resolver = StateResolver::new(Some(default_state));

        let resolved = resolver.resolve(Some(target_path.clone())).unwrap();
        let search_output = synrepo::surface::mcp::search::handle_search(
            &resolved,
            "target_needle_unique".into(),
            5,
        );
        assert!(
            search_output.contains("target_needle_unique"),
            "{search_output}"
        );
        assert!(!search_output.contains("default_needle"), "{search_output}");

        let file_id = file_id_for(&target_path, "src/lib.rs");
        let node_output =
            synrepo::surface::mcp::primitives::handle_node(&resolved, file_id.to_string());
        assert!(
            node_output.contains("\"node_type\": \"file\""),
            "{node_output}"
        );
        assert!(!node_output.contains("\"error\""), "{node_output}");
    }

    #[test]
    fn mcp_resolution_does_not_auto_start_watch() {
        let _home = home_fixture();
        let (_default_repo, default_path) = ready_repo("pub fn default_watch_free() {}\n");
        let (_target_repo, target_path) = ready_repo("pub fn target_watch_free() {}\n");
        registry::record_project(&target_path).unwrap();
        let default_state = prepare_state(&default_path).unwrap();
        let server = SynrepoServer::new_optional(Some(default_state), false);

        let default_watch = watch_service_status(&Config::synrepo_dir(&default_path));
        assert!(matches!(default_watch, WatchServiceStatus::Inactive));

        server.resolve_state(Some(target_path.clone())).unwrap();

        let target_watch = watch_service_status(&Config::synrepo_dir(&target_path));
        assert!(matches!(target_watch, WatchServiceStatus::Inactive));
    }

    #[test]
    fn unregistered_requested_repo_is_rejected() {
        let _home = home_fixture();
        let dir = tempdir().unwrap();
        let resolver = StateResolver::new(None);

        let err = resolve_error(resolver.resolve(Some(dir.path().to_path_buf()))).to_string();
        assert!(err.contains("not managed by synrepo"), "{err}");
        assert!(err.contains("synrepo project add"), "{err}");
    }

    #[test]
    fn corrupt_registry_is_not_treated_as_empty() {
        let _home = home_fixture();
        let dir = tempdir().unwrap();
        let registry_path = registry::registry_path().unwrap();
        fs::create_dir_all(registry_path.parent().unwrap()).unwrap();
        fs::write(&registry_path, "not valid = @@@").unwrap();
        let resolver = StateResolver::new(None);

        let err = resolve_error(resolver.resolve(Some(dir.path().to_path_buf())));
        assert!(format!("{err:#}").contains("failed to parse registry"));
    }

    #[test]
    fn requested_prepare_failure_does_not_fallback_to_default() {
        let _home = home_fixture();
        let (_default_repo, default_path) = ready_repo("pub fn default_only() {}\n");
        let bad_repo = tempdir().unwrap();
        registry::record_project(bad_repo.path()).unwrap();
        let default_state = prepare_state(&default_path).unwrap();
        let resolver = StateResolver::new(Some(default_state));

        let err = resolve_error(resolver.resolve(Some(bad_repo.path().to_path_buf()))).to_string();
        assert!(err.contains("failed to prepare"), "{err}");
        assert!(err.contains("synrepo init"), "{err}");
    }

    fn file_id_for(repo_root: &Path, path: &str) -> synrepo::FileNodeId {
        let graph_dir = Config::synrepo_dir(repo_root).join("graph");
        let store = SqliteGraphStore::open_existing(&graph_dir).unwrap();
        store.file_by_path(path).unwrap().unwrap().id
    }
}
