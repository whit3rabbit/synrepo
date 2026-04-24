mod config;
mod mcp_register;
mod orchestration;
mod report;
mod steps;

pub(crate) use config::{load_json_config, write_json_config};
pub(crate) use orchestration::{
    agent_setup_many_resolved, resolve_tool_resolution, setup_many_resolved,
};
#[cfg(test)]
pub(crate) use report::{
    classify_mcp_registration, classify_shim_freshness, entry_after_failure, entry_after_success,
    render_client_setup_summary, ClientBefore, ClientSetupEntry, McpRegistration, ShimFreshness,
};
pub(crate) use steps::{
    step_apply_integration, step_ensure_ready, step_init, step_register_mcp, step_write_shim,
    StepOutcome,
};

#[cfg(test)]
pub(crate) use mcp_register::{
    setup_claude_mcp, setup_codex_mcp, setup_cursor_mcp, setup_opencode_mcp, setup_roo_mcp,
    setup_windsurf_mcp,
};
