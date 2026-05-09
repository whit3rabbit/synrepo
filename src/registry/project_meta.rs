use std::path::{Path, PathBuf};

use crate::pipeline::writer::now_rfc3339;

use super::{
    canonicalize, default_synrepo_dir, io, registry_path, AgentEntry, AgentHookEntry, HookEntry,
    ProjectEntry, Registry, SCHEMA_VERSION,
};

/// Derive the stable registry project ID for a canonical project path.
pub fn derive_project_id(path: &Path) -> String {
    let hash = blake3::hash(path.to_string_lossy().as_bytes());
    let hex = hex::encode(&hash.as_bytes()[..8]);
    format!("proj_{hex}")
}

/// Return the default display name for a project path.
pub fn default_project_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("project")
        .to_string()
}

impl ProjectEntry {
    /// Return the persisted ID or the legacy derived ID.
    pub fn effective_id(&self) -> String {
        if self.id.is_empty() {
            derive_project_id(&self.path)
        } else {
            self.id.clone()
        }
    }

    /// Return the display alias or the path-derived default name.
    pub fn display_name(&self) -> String {
        self.name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_project_name(&self.path))
    }

    /// True when the project was explicitly registered for user-facing
    /// project management, not merely recorded as bootstrap bookkeeping.
    pub fn is_explicitly_registered(&self) -> bool {
        self.last_opened_at.is_some()
            || self
                .name
                .as_deref()
                .map(|name| !name.trim().is_empty())
                .unwrap_or(false)
            || !self.agents.is_empty()
            || !self.hooks.is_empty()
            || !self.agent_hooks.is_empty()
    }
}

pub(super) fn new_project_entry(path: PathBuf, root_gitignore_entry_added: bool) -> ProjectEntry {
    ProjectEntry {
        id: derive_project_id(&path),
        path,
        name: None,
        last_opened_at: None,
        initialized_at: now_rfc3339(),
        synrepo_dir: default_synrepo_dir(),
        root_gitignore_entry_added,
        export_gitignore_entry_added: false,
        export_gitignore_entry: None,
        agents: Vec::<AgentEntry>::new(),
        hooks: Vec::<HookEntry>::new(),
        agent_hooks: Vec::<AgentHookEntry>::new(),
    }
}

pub(super) fn ensure_project_identity(entry: &mut ProjectEntry) {
    if entry.id.is_empty() {
        entry.id = derive_project_id(&entry.path);
    }
}

#[derive(Debug)]
/// Error returned when a project selector cannot resolve to one registry row.
pub struct ProjectResolutionError {
    message: String,
}

impl ProjectResolutionError {
    fn not_found(selector: &str) -> Self {
        Self {
            message: format!("no managed project found for `{selector}`"),
        }
    }

    fn ambiguous(selector: &str, matches: &[&ProjectEntry]) -> Self {
        let mut message = format!("multiple projects match `{selector}`:");
        for entry in matches {
            message.push_str(&format!(
                "\n  {}  {}  {}",
                entry.effective_id(),
                entry.display_name(),
                entry.path.display()
            ));
        }
        Self { message }
    }
}

impl std::fmt::Display for ProjectResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProjectResolutionError {}

pub(super) fn resolve_project_index(
    registry: &Registry,
    selector: &str,
    canonical_path: Option<&Path>,
) -> Result<usize, ProjectResolutionError> {
    if let Some(idx) = registry
        .projects
        .iter()
        .position(|entry| entry.effective_id() == selector)
    {
        return Ok(idx);
    }

    if let Some(path) = canonical_path {
        if let Some(idx) = registry
            .projects
            .iter()
            .position(|entry| entry.path == path)
        {
            return Ok(idx);
        }
    }

    let matches: Vec<&ProjectEntry> = registry
        .projects
        .iter()
        .filter(|entry| entry.display_name() == selector)
        .collect();
    match matches.as_slice() {
        [entry] => registry
            .projects
            .iter()
            .position(|candidate| candidate.path == entry.path)
            .ok_or_else(|| ProjectResolutionError::not_found(selector)),
        [] => Err(ProjectResolutionError::not_found(selector)),
        many => Err(ProjectResolutionError::ambiguous(selector, many)),
    }
}

/// Resolve a managed project by stable ID, display name, or path string.
pub fn resolve_project(selector: &str) -> anyhow::Result<ProjectEntry> {
    let registry = super::load()?;
    let canonical = selector_path(selector).map(|path| canonicalize(&path));
    let idx = resolve_project_index(&registry, selector, canonical.as_deref())?;
    Ok(registry.projects[idx].clone())
}

/// Mark a managed project as recently opened and return the updated entry.
pub fn mark_project_opened(selector: &str) -> anyhow::Result<ProjectEntry> {
    let Some(path) = registry_path() else {
        anyhow::bail!("cannot write registry: no home directory detected");
    };
    let mut registry = io::load_from(&path)?;
    registry.schema_version = SCHEMA_VERSION;
    let canonical = selector_path(selector).map(|path| canonicalize(&path));
    let idx = resolve_project_index(&registry, selector, canonical.as_deref())?;
    let entry = &mut registry.projects[idx];
    ensure_project_identity(entry);
    entry.last_opened_at = Some(now_rfc3339());
    let updated = entry.clone();
    io::save_to(&path, &registry)?;
    Ok(updated)
}

/// Rename a managed project's display alias without changing project storage.
pub fn rename_project(selector: &str, name: &str) -> anyhow::Result<ProjectEntry> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("project name cannot be empty");
    }
    let Some(path) = registry_path() else {
        anyhow::bail!("cannot write registry: no home directory detected");
    };
    let mut registry = io::load_from(&path)?;
    registry.schema_version = SCHEMA_VERSION;
    let canonical = selector_path(selector).map(|path| canonicalize(&path));
    let idx = resolve_project_index(&registry, selector, canonical.as_deref())?;
    let entry = &mut registry.projects[idx];
    ensure_project_identity(entry);
    entry.name = Some(trimmed.to_string());
    let updated = entry.clone();
    io::save_to(&path, &registry)?;
    Ok(updated)
}

fn selector_path(selector: &str) -> Option<PathBuf> {
    let path = Path::new(selector);
    if path.is_absolute() || selector.contains('/') || selector.contains('\\') || path.exists() {
        Some(path.to_path_buf())
    } else {
        None
    }
}
