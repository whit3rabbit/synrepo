use std::path::PathBuf;

use crate::{
    core::path_safety::resolve_existing_path_in_repo,
    substrate::{discover_roots, DiscoveryRoot},
    surface::mcp::{error::McpError, SynrepoState},
};

pub(super) const PRIMARY_ROOT_ID: &str = "primary";

#[derive(Clone, Debug)]
pub(super) struct ResolvedEditPath {
    pub(super) root_id: String,
    pub(super) is_primary_root: bool,
    pub(super) relative: String,
    pub(super) absolute: PathBuf,
}

pub(super) fn resolve_edit_path(
    state: &SynrepoState,
    input: &str,
    root_id: Option<&str>,
) -> anyhow::Result<ResolvedEditPath> {
    let root = edit_root(state, root_id)?;
    let resolved = resolve_existing_path_in_repo(&root.absolute_path, input)
        .map_err(|err| anyhow::anyhow!(err))?;
    Ok(ResolvedEditPath {
        root_id: root.discriminant.clone(),
        is_primary_root: root.discriminant == PRIMARY_ROOT_ID,
        relative: resolved.relative,
        absolute: resolved.absolute,
    })
}

pub(super) fn require_matching_root(expected: Option<&str>, actual: &str) -> anyhow::Result<()> {
    if expected.is_some_and(|expected| expected != actual) {
        return Err(McpError::invalid_parameter(format!(
            "root_id does not match resolved graph target: expected {expected:?}, actual {actual}"
        ))
        .into());
    }
    Ok(())
}

fn edit_root(state: &SynrepoState, root_id: Option<&str>) -> anyhow::Result<DiscoveryRoot> {
    let requested = root_id.unwrap_or(PRIMARY_ROOT_ID);
    discover_roots(&state.repo_root, &state.config)
        .into_iter()
        .find(|root| root.discriminant == requested)
        .ok_or_else(|| {
            McpError::invalid_parameter(format!("unknown root_id for edit target: {requested}"))
                .into()
        })
}
