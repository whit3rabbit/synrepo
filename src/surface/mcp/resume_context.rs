use schemars::JsonSchema;
use serde::Deserialize;

use crate::surface::resume_context::{
    build_resume_context, ResumeContextRequest, DEFAULT_RESUME_CONTEXT_LIMIT,
    DEFAULT_RESUME_CONTEXT_SINCE_DAYS, DEFAULT_RESUME_CONTEXT_TOKEN_CAP,
};

use super::helpers::render_result;
use super::SynrepoState;

fn default_limit() -> usize {
    DEFAULT_RESUME_CONTEXT_LIMIT
}

fn default_since_days() -> u32 {
    DEFAULT_RESUME_CONTEXT_SINCE_DAYS
}

fn default_budget_tokens() -> usize {
    DEFAULT_RESUME_CONTEXT_TOKEN_CAP
}

fn default_include_notes() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResumeContextParams {
    pub repo_root: Option<std::path::PathBuf>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_since_days")]
    pub since_days: u32,
    #[serde(default = "default_budget_tokens")]
    pub budget_tokens: usize,
    #[serde(default = "default_include_notes")]
    pub include_notes: bool,
}

pub fn handle_resume_context(state: &SynrepoState, params: ResumeContextParams) -> String {
    render_result((|| {
        let packet = build_resume_context(
            &state.repo_root,
            &state.config,
            ResumeContextRequest {
                limit: params.limit,
                since_days: params.since_days,
                budget_tokens: params.budget_tokens,
                include_notes: params.include_notes,
            },
        )?;
        Ok(serde_json::to_value(packet)?)
    })())
}
