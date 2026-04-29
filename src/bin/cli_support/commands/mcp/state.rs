use std::collections::HashMap;
use std::collections::HashSet;
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

    pub(crate) fn default_repo_root(&self) -> Option<PathBuf> {
        self.default_repo_root.clone()
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
            auto_started_roots: Arc::clone(&self.auto_started_roots),
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
        let auto_started_roots = HashSet::new();
        let mut tool_router = Self::build_tool_router();
        if !allow_edits {
            tool_router.remove_route("synrepo_prepare_edit_context");
            tool_router.remove_route("synrepo_apply_anchor_edits");
        }
        let server = Self {
            resolver,
            auto_started_roots: Arc::new(RwLock::new(auto_started_roots)),
            tool_router,
            allow_edits,
        };

        if let Some(repo_root) = server.resolver.default_repo_root() {
            server.maybe_auto_start_watch(&repo_root);
        }

        server
    }

    pub(super) fn resolve_state(
        &self,
        param_root: Option<PathBuf>,
    ) -> anyhow::Result<Arc<SynrepoState>> {
        let state = self.resolver.resolve(param_root)?;
        self.maybe_auto_start_watch(&state.repo_root);
        Ok(state)
    }

    pub(super) fn with_state<F>(&self, param_root: Option<PathBuf>, f: F) -> String
    where
        F: FnOnce(Arc<SynrepoState>) -> String,
    {
        match self.resolve_state(param_root) {
            Ok(state) => f(state),
            Err(error) => render_state_error(error),
        }
    }

    fn maybe_auto_start_watch(&self, repo_root: &Path) {
        if self.auto_started_roots.read().contains(repo_root) {
            return;
        }
        if let Ok(Some(_)) = super::super::watch::maybe_spawn_watch_daemon(repo_root) {
            self.auto_started_roots
                .write()
                .insert(repo_root.to_path_buf());
        }
    }

    /// Stop all watch daemons that were auto-started by this server instance.
    pub(crate) fn stop_auto_started_watchers(&self) {
        let mut roots = self.auto_started_roots.write();
        for root in roots.drain() {
            let _ = super::super::watch::watch_stop(&root);
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

    #[cfg(test)]
    pub(crate) fn registered_tool_names(&self) -> Vec<String> {
        self.tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect()
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
