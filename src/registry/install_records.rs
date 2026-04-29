//! Registry write helpers for installed hooks and binary metadata.

use std::path::Path;

use crate::pipeline::writer::now_rfc3339;

use super::{
    canonicalize, default_synrepo_dir, find_project_mut, io, registry_path, BinaryEntry, HookEntry,
    ProjectEntry, SCHEMA_VERSION,
};

/// Record Git hooks installed or updated for a project.
pub fn record_hooks(project: &Path, hooks: Vec<HookEntry>) -> anyhow::Result<()> {
    let Some(path) = registry_path() else {
        return Ok(());
    };
    let mut registry = io::load_from(&path)?;
    registry.schema_version = SCHEMA_VERSION;
    let canonical = canonicalize(project);
    let entry = match registry.projects.iter_mut().find(|p| p.path == canonical) {
        Some(e) => {
            if e.id.is_empty() {
                e.id = super::derive_project_id(&e.path);
            }
            e
        }
        None => {
            registry.projects.push(ProjectEntry {
                id: super::derive_project_id(&canonical),
                path: canonical,
                name: None,
                last_opened_at: None,
                initialized_at: now_rfc3339(),
                synrepo_dir: default_synrepo_dir(),
                root_gitignore_entry_added: false,
                export_gitignore_entry_added: false,
                agents: Vec::new(),
                hooks: Vec::new(),
            });
            registry
                .projects
                .last_mut()
                .expect("just pushed an entry; vec is non-empty")
        }
    };
    for hook in hooks {
        match entry.hooks.iter_mut().find(|h| h.name == hook.name) {
            Some(existing) => *existing = hook,
            None => entry.hooks.push(hook),
        }
    }
    io::save_to(&path, &registry)
}

/// Drop hook records from a project after uninstall.
pub fn record_hooks_uninstall(project: &Path, names: &[String]) -> anyhow::Result<()> {
    let Some(path) = registry_path() else {
        return Ok(());
    };
    let mut registry = io::load_from(&path)?;
    let canonical = canonicalize(project);
    if let Some(entry) = find_project_mut(&mut registry, &canonical) {
        entry
            .hooks
            .retain(|h| !names.iter().any(|name| name == &h.name));
        io::save_to(&path, &registry)?;
    }
    Ok(())
}

/// Record uninstall progress without dropping project data ownership early.
pub fn record_uninstall_progress(
    project: &Path,
    agent_tools: &[String],
    hook_names: &[String],
    root_gitignore_removed: bool,
    export_gitignore_removed: bool,
    project_data_deleted: bool,
) -> anyhow::Result<()> {
    let Some(path) = registry_path() else {
        return Ok(());
    };
    let mut registry = io::load_from(&path)?;
    let canonical = canonicalize(project);
    let Some(entry) = find_project_mut(&mut registry, &canonical) else {
        return Ok(());
    };

    entry
        .agents
        .retain(|agent| !agent_tools.iter().any(|tool| tool == &agent.tool));
    entry
        .hooks
        .retain(|hook| !hook_names.iter().any(|name| name == &hook.name));
    if root_gitignore_removed {
        entry.root_gitignore_entry_added = false;
    }
    if export_gitignore_removed {
        entry.export_gitignore_entry_added = false;
    }

    if project_data_deleted
        && entry.agents.is_empty()
        && entry.hooks.is_empty()
        && !entry.root_gitignore_entry_added
        && !entry.export_gitignore_entry_added
    {
        registry.projects.retain(|p| p.path != canonical);
    }

    io::save_to(&path, &registry)
}

/// Record the installed binary location when an installer can determine it.
pub fn record_binary(binary: BinaryEntry) -> anyhow::Result<()> {
    let Some(path) = registry_path() else {
        return Ok(());
    };
    let mut registry = io::load_from(&path)?;
    registry.schema_version = SCHEMA_VERSION;
    registry.binary = Some(binary);
    io::save_to(&path, &registry)
}

/// Drop the binary install record after uninstall guidance or direct deletion.
pub fn record_binary_uninstall() -> anyhow::Result<()> {
    let Some(path) = registry_path() else {
        return Ok(());
    };
    let mut registry = io::load_from(&path)?;
    registry.binary = None;
    io::save_to(&path, &registry)
}
