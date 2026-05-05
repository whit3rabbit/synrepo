use std::path::PathBuf;
use std::sync::Arc;

use rmcp::{
    model::{ReadResourceResult, ResourceContents},
    ErrorData as RmcpError,
};
use synrepo::surface::mcp::{
    context_pack,
    error::{self, ErrorCode, McpError},
    SynrepoState,
};

use super::SynrepoServer;

const RESOURCE_TOOL: &str = "mcp_resource";

impl SynrepoServer {
    pub(super) async fn read_resource_with_controls(
        &self,
        uri: String,
    ) -> Result<ReadResourceResult, RmcpError> {
        let server = self.clone();
        let timeout = self.call_timeout;
        let task = tokio::task::spawn_blocking(move || server.read_resource_blocking(uri));
        match tokio::time::timeout(timeout, task).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(to_rmcp_error(
                McpError::internal(format!("MCP resource task failed: {error}")).into(),
            )),
            Err(_) => Err(to_rmcp_error(
                McpError::timeout(format!(
                    "MCP resource read exceeded {}s timeout",
                    timeout.as_secs()
                ))
                .into(),
            )),
        }
    }

    fn read_resource_blocking(&self, uri: String) -> Result<ReadResourceResult, RmcpError> {
        if let Err(error) = self.session.check_rate_limit(RESOURCE_TOOL) {
            self.session.record_tool(RESOURCE_TOOL, true);
            return Err(to_rmcp_error(error));
        }

        maybe_delay_resource_for_test();

        let result = self.read_resource_text(&uri);
        let errored = result.is_err();
        self.session.record_tool(RESOURCE_TOOL, errored);
        match result {
            Ok((state, text)) => {
                if let Some(state) = state {
                    self.record_resource_for(&state);
                }
                Ok(ReadResourceResult::new(vec![ResourceContents::text(
                    text, uri,
                )
                .with_mime_type("application/json")]))
            }
            Err(error) => Err(to_rmcp_error(error)),
        }
    }

    fn read_resource_text(&self, uri: &str) -> anyhow::Result<(Option<Arc<SynrepoState>>, String)> {
        if uri == "synrepo://projects" {
            let registry = synrepo::registry::load()?;
            let text = serde_json::to_string_pretty(&registry)?;
            return Ok((None, text));
        }

        if let Some((project_id, resource_uri)) = project_resource_uri(uri)? {
            let root = project_root_by_id(&project_id)?;
            let state = self.resolve_state(Some(root))?;
            let text =
                context_pack::read_resource(&state, &resource_uri).map_err(McpError::not_found)?;
            return Ok((Some(state), text));
        }

        let state = self.resolve_state(None)?;
        let text = context_pack::read_resource(&state, uri).map_err(McpError::not_found)?;
        Ok((Some(state), text))
    }
}

fn project_resource_uri(uri: &str) -> anyhow::Result<Option<(String, String)>> {
    let Some(rest) = uri.strip_prefix("synrepo://project/") else {
        return Ok(None);
    };
    let Some((project_id, resource)) = rest.split_once('/') else {
        return Err(McpError::invalid_parameter(format!(
            "project-qualified resource URI must include a resource path: {uri}"
        ))
        .into());
    };
    if project_id.is_empty() || resource.is_empty() {
        return Err(McpError::invalid_parameter(format!(
            "project-qualified resource URI must include project_id and resource path: {uri}"
        ))
        .into());
    }
    Ok(Some((
        decode_resource_component(project_id),
        format!("synrepo://{resource}"),
    )))
}

fn project_root_by_id(project_id: &str) -> anyhow::Result<PathBuf> {
    let registry = synrepo::registry::load()?;
    registry
        .projects
        .into_iter()
        .find(|entry| entry.effective_id() == project_id)
        .map(|entry| entry.path)
        .ok_or_else(|| {
            McpError::not_found(format!("managed project not found for id: {project_id}")).into()
        })
}

fn to_rmcp_error(error: anyhow::Error) -> RmcpError {
    let code = error::classify_error(&error);
    let data = error::error_value(&error);
    let message = data
        .pointer("/error/message")
        .and_then(|value| value.as_str())
        .unwrap_or("MCP resource error")
        .to_string();
    match code {
        ErrorCode::InvalidParameter => RmcpError::invalid_params(message, Some(data)),
        ErrorCode::NotFound | ErrorCode::NotInitialized => {
            RmcpError::resource_not_found(message, Some(data))
        }
        ErrorCode::RateLimited
        | ErrorCode::Locked
        | ErrorCode::Busy
        | ErrorCode::Timeout
        | ErrorCode::Internal => RmcpError::internal_error(message, Some(data)),
    }
}

fn decode_resource_component(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(value) = u8::from_str_radix(hex, 16) {
                    out.push(value);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
fn maybe_delay_resource_for_test() {
    let ms = RESOURCE_DELAY_MS.load(std::sync::atomic::Ordering::SeqCst);
    if ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

#[cfg(not(test))]
fn maybe_delay_resource_for_test() {}

#[cfg(test)]
static RESOURCE_DELAY_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    use rmcp::model::ResourceContents;
    use tempfile::{tempdir, TempDir};

    use super::*;
    use crate::cli_support::commands::mcp_runtime::prepare_state;
    use synrepo::{bootstrap::bootstrap, config::test_home, registry};

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

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().unwrap()
    }

    fn text(result: &ReadResourceResult) -> &str {
        match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            ResourceContents::BlobResourceContents { .. } => panic!("expected text resource"),
        }
    }

    struct ResourceDelayGuard;

    impl ResourceDelayGuard {
        fn set(ms: u64) -> Self {
            RESOURCE_DELAY_MS.store(ms, std::sync::atomic::Ordering::SeqCst);
            Self
        }
    }

    impl Drop for ResourceDelayGuard {
        fn drop(&mut self) {
            RESOURCE_DELAY_MS.store(0, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[test]
    fn resource_reads_default_repo_and_records_session() {
        let _home = home_fixture();
        let (_repo, repo_path) = ready_repo("pub fn default_resource_needle() {}\n");
        let state = prepare_state(&repo_path).unwrap();
        let server = SynrepoServer::new_optional(Some(state), false);

        let result = runtime()
            .block_on(
                server.read_resource_with_controls("synrepo://file/src/lib.rs/outline".to_string()),
            )
            .unwrap();
        let body = text(&result);
        assert!(body.contains("default_resource_needle"), "{body}");

        let metrics = server.metrics_for_repo_root(None);
        let value: serde_json::Value = serde_json::from_str(&metrics).unwrap();
        assert_eq!(value["this_session"]["calls_by_tool"][RESOURCE_TOOL], 1);
    }

    #[test]
    fn resource_reads_project_qualified_repo() {
        let _home = home_fixture();
        let (_default_repo, default_path) = ready_repo("pub fn default_resource_only() {}\n");
        let (_target_repo, target_path) = ready_repo("pub fn target_resource_needle() {}\n");
        let entry = registry::record_project(&target_path).unwrap();
        let default_state = prepare_state(&default_path).unwrap();
        let server = SynrepoServer::new_optional(Some(default_state), false);

        let result = runtime()
            .block_on(server.read_resource_with_controls(format!(
                "synrepo://project/{}/file/src/lib.rs/outline",
                entry.effective_id()
            )))
            .unwrap();
        let body = text(&result);

        assert!(body.contains("target_resource_needle"), "{body}");
        assert!(!body.contains("default_resource_only"), "{body}");
    }

    #[test]
    fn resource_unknown_project_id_returns_not_found() {
        let _home = home_fixture();
        let server = SynrepoServer::new_optional(None, false);

        let err = runtime()
            .block_on(server.read_resource_with_controls(
                "synrepo://project/proj_missing/card/src/lib.rs".to_string(),
            ))
            .unwrap_err();

        assert!(err.message.contains("managed project not found"), "{err:?}");
    }

    #[test]
    fn resource_corrupt_registry_is_loud() {
        let _home = home_fixture();
        let registry_path = registry::registry_path().unwrap();
        fs::create_dir_all(registry_path.parent().unwrap()).unwrap();
        fs::write(&registry_path, "not valid = @@@").unwrap();
        let server = SynrepoServer::new_optional(None, false);

        let err = runtime()
            .block_on(server.read_resource_with_controls("synrepo://projects".to_string()))
            .unwrap_err();

        assert!(err.message.contains("failed to parse registry"), "{err:?}");
    }

    #[test]
    fn resource_read_respects_call_timeout() {
        let _home = home_fixture();
        let _delay = ResourceDelayGuard::set(50);
        let server =
            SynrepoServer::new_optional_with_timeout(None, false, false, Duration::from_millis(1));

        let err = runtime()
            .block_on(server.read_resource_with_controls("synrepo://projects".to_string()))
            .unwrap_err();

        assert!(err.message.contains("timeout"), "{err:?}");
    }
}
