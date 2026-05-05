//! `synrepo mcp` subcommand — starts the MCP server over stdio.

mod state;
mod tools;

use std::future::Future;
use std::time::Duration;

use rmcp::{
    handler::server::router::tool::ToolRouter,
    model::{
        ListResourceTemplatesResult, PaginatedRequestParams, RawResourceTemplate,
        ReadResourceRequestParams, ReadResourceResult, ResourceContents, ResourceTemplate,
        ServerCapabilities, ServerInfo,
    },
    service::{RequestContext, RoleServer},
    tool_handler, ErrorData as McpError, ServerHandler,
};
use synrepo::surface::mcp::context_pack;

use state::{SessionState, StateResolver};

pub(crate) struct SynrepoServer {
    resolver: StateResolver,
    tool_router: ToolRouter<Self>,
    allow_overlay_writes: bool,
    allow_source_edits: bool,
    session: SessionState,
    call_timeout: Duration,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SynrepoServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_instructions(
            "synrepo provides structured code-intelligence context for codebase questions, file reviews, search routing, impact checks, and edits. \
             Required workflow: synrepo_orient to start, synrepo_find to route a task, \
             synrepo_explain for bounded details, synrepo_impact (or its shorthand synrepo_risks) before edits, \
             synrepo_tests before claiming done, and synrepo_changed after edits. \
             Use synrepo_minimum_context as the bounded neighborhood step once a focal target is known. \
             For broad lexical searches, pass output_mode=\"compact\" to get grouped, token-accounted routing output, or output_mode=\"cards\" to get tiny file cards directly. \
             Use synrepo_context_pack or synrepo_card targets=[...] when batching several read-only context artifacts is cheaper than serial tool calls. \
             Global MCP configs serve registered projects by absolute path: pass the current workspace as repo_root; \
             call synrepo_use_project once when a global/defaultless session should remember a default repo. \
             if a repository is not managed, ask the user to run synrepo project add <path>. \
             Repo-bound configs launched with synrepo mcp --repo . may omit repo_root. \
             Overlay write tools are absent unless this server was started with synrepo mcp --allow-overlay-writes. \
             Source edit tools are absent unless started with synrepo mcp --allow-source-edits; when present, call prepare before apply. \
             Tool errors are structured with error.code and transitional error_message; synrepo_metrics exposes this-session and persisted usage counters. \
             Read full source files only after card routing identifies the target or when a bounded card is insufficient; \
             that is an explicit escalation, not the default first step. \
             Graph-backed structural facts are authoritative; overlay commentary and advisory notes never define source truth. \
             Existing task-first, audit, overlay, and graph primitive tools remain available.",
        )
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourceTemplatesResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new(
                RawResourceTemplate::new("synrepo://card/{target}", "synrepo card")
                    .with_description("Read a card-shaped JSON context artifact.")
                    .with_mime_type("application/json"),
                None,
            ),
            ResourceTemplate::new(
                RawResourceTemplate::new("synrepo://file/{path}/outline", "synrepo file outline")
                    .with_description("Read a compact file outline with symbols and hashes.")
                    .with_mime_type("application/json"),
                None,
            ),
            ResourceTemplate::new(
                RawResourceTemplate::new(
                    "synrepo://context-pack?goal={goal}",
                    "synrepo context pack",
                )
                .with_description("Read a batched read-only context pack.")
                .with_mime_type("application/json"),
                None,
            ),
            ResourceTemplate::new(
                RawResourceTemplate::new("synrepo://projects", "synrepo managed projects")
                    .with_description("List managed projects from the user-level registry.")
                    .with_mime_type("application/json"),
                None,
            ),
        ])))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        let uri = request.uri;
        let server = self.clone();
        async move {
            match tokio::task::spawn_blocking(move || server.read_resource_blocking(uri)).await {
                Ok(result) => result,
                Err(error) => Err(McpError::internal_error(
                    format!("MCP resource task failed: {error}"),
                    None,
                )),
            }
        }
    }
}

impl SynrepoServer {
    fn read_resource_blocking(&self, uri: String) -> Result<ReadResourceResult, McpError> {
        if uri == "synrepo://projects" {
            let text = match synrepo::registry::load()
                .and_then(|registry| serde_json::to_string_pretty(&registry).map_err(Into::into))
            {
                Ok(text) => text,
                Err(error) => {
                    return Err(McpError::resource_not_found(error.to_string(), None));
                }
            };
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text, uri,
            )
            .with_mime_type("application/json")]));
        }
        let state = match self.resolve_state(None) {
            Ok(state) => state,
            Err(error) => {
                return Err(McpError::resource_not_found(error.to_string(), None));
            }
        };
        match context_pack::read_resource(&state, &uri) {
            Ok(text) => {
                self.record_resource_for(&state);
                Ok(ReadResourceResult::new(vec![ResourceContents::text(
                    text, uri,
                )
                .with_mime_type("application/json")]))
            }
            Err(message) => Err(McpError::resource_not_found(message, None)),
        }
    }
}
