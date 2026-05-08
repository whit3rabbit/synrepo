//! Repo resume-context command implementation.

use std::path::Path;

use synrepo::{
    config::Config,
    surface::resume_context::{
        build_resume_context, to_json, to_markdown, ResumeContextRequest,
        DEFAULT_RESUME_CONTEXT_LIMIT, DEFAULT_RESUME_CONTEXT_SINCE_DAYS,
        DEFAULT_RESUME_CONTEXT_TOKEN_CAP,
    },
};

pub(crate) fn resume_context(
    repo_root: &Path,
    limit: Option<usize>,
    since_days: Option<u32>,
    budget_tokens: Option<usize>,
    no_notes: bool,
    json: bool,
) -> anyhow::Result<()> {
    print!(
        "{}",
        resume_context_output(repo_root, limit, since_days, budget_tokens, no_notes, json)?
    );
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn resume_context_output(
    repo_root: &Path,
    limit: Option<usize>,
    since_days: Option<u32>,
    budget_tokens: Option<usize>,
    no_notes: bool,
    json: bool,
) -> anyhow::Result<String> {
    let config = Config::load(repo_root)?;
    let packet = build_resume_context(
        repo_root,
        &config,
        ResumeContextRequest {
            limit: limit.unwrap_or(DEFAULT_RESUME_CONTEXT_LIMIT),
            since_days: since_days.unwrap_or(DEFAULT_RESUME_CONTEXT_SINCE_DAYS),
            budget_tokens: budget_tokens.unwrap_or(DEFAULT_RESUME_CONTEXT_TOKEN_CAP),
            include_notes: !no_notes,
        },
    )?;
    let mut output = if json {
        to_json(&packet)
    } else {
        to_markdown(&packet)
    };
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}
