pub(crate) mod agent_shims;
pub(crate) mod apply_report;
pub(crate) mod cli_args;
pub(crate) mod commands;
pub(crate) mod entry;
pub(crate) mod explain_cmd;
mod graph;
pub(crate) mod repair_cmd;
pub(crate) mod setup_cmd;
pub(crate) mod setup_dispatch;
pub(crate) mod setup_plan;

#[cfg(test)]
mod tests;
