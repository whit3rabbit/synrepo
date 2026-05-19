//! Registry write helpers for installed hooks and binary metadata.

use std::path::Path;

use crate::pipeline::writer::now_rfc3339;

use super::{
    canonicalize, default_synrepo_dir, find_project_mut, io, new_project_entry, registry_path,
    AgentHookEntry, BinaryEntry, HookEntry, ProjectEntry, SCHEMA_VERSION,
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
                syntext_gitignore_entry_added: false,
                export_gitignore_entry_added: false,
                export_gitignore_entry: None,
                agents: Vec::new(),
                hooks: Vec::new(),
                agent_hooks: Vec::new(),
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

/// Record local client-side nudge hooks installed or updated for a project.
pub fn record_agent_hooks(project: &Path, hooks: Vec<AgentHookEntry>) -> anyhow::Result<()> {
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
                syntext_gitignore_entry_added: false,
                export_gitignore_entry_added: false,
                export_gitignore_entry: None,
                agents: Vec::new(),
                hooks: Vec::new(),
                agent_hooks: Vec::new(),
            });
            registry
                .projects
                .last_mut()
                .expect("just pushed an entry; vec is non-empty")
        }
    };
    for hook in hooks {
        match entry.agent_hooks.iter_mut().find(|h| h.tool == hook.tool) {
            Some(existing) => *existing = hook,
            None => entry.agent_hooks.push(hook),
        }
    }
    io::save_to(&path, &registry)
}

/// Mark that synrepo appended `.syntext/` to the root `.gitignore`.
///
/// This flag is OR-recorded and only cleared after uninstall removes the line.
pub fn record_syntext_gitignore(project: &Path, added: bool) -> anyhow::Result<()> {
    if !added {
        return Ok(());
    }
    let Some(path) = registry_path() else {
        return Ok(());
    };
    let mut registry = io::load_from(&path)?;
    registry.schema_version = SCHEMA_VERSION;
    let canonical = canonicalize(project);
    match find_project_mut(&mut registry, &canonical) {
        Some(entry) => {
            if entry.id.is_empty() {
                entry.id = super::derive_project_id(&entry.path);
            }
            entry.syntext_gitignore_entry_added = true;
        }
        None => {
            let mut entry = new_project_entry(canonical, false);
            entry.syntext_gitignore_entry_added = true;
            registry.projects.push(entry);
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

/// Drop a single agent's record from a project entry.
pub fn record_agent_uninstall(project: &Path, tool: &str) -> anyhow::Result<()> {
    let Some(path) = registry_path() else {
        return Ok(());
    };
    let mut registry = io::load_from(&path)?;
    let canonical = canonicalize(project);
    if let Some(entry) = find_project_mut(&mut registry, &canonical) {
        if entry.id.is_empty() {
            entry.id = super::derive_project_id(&entry.path);
        }
        entry.agents.retain(|a| a.tool != tool);
        io::save_to(&path, &registry)?;
    }
    Ok(())
}

/// Drop a project entry entirely (`synrepo remove` bulk path).
pub fn record_uninstall(project: &Path) -> anyhow::Result<()> {
    let Some(path) = registry_path() else {
        return Ok(());
    };
    let mut registry = io::load_from(&path)?;
    let canonical = canonicalize(project);
    registry.projects.retain(|p| p.path != canonical);
    io::save_to(&path, &registry)
}

/// Completed uninstall work that can clear registry ownership metadata.
pub struct UninstallProgress<'a> {
    /// Agent tool names whose registry records were removed successfully.
    pub agent_tools: &'a [String],
    /// Git hook names removed successfully.
    pub hook_names: &'a [String],
    /// Agent-hook tool names removed successfully.
    pub agent_hook_tools: &'a [String],
    /// Whether the `.synrepo/` root `.gitignore` line was removed.
    pub root_gitignore_removed: bool,
    /// Whether the `.syntext/` root `.gitignore` line was removed.
    pub syntext_gitignore_removed: bool,
    /// Whether the export root `.gitignore` line was removed.
    pub export_gitignore_removed: bool,
    /// Whether project-local `.synrepo/` runtime data was deleted.
    pub project_data_deleted: bool,
}

/// Record uninstall progress without dropping project data ownership early.
pub fn record_uninstall_progress(
    project: &Path,
    progress: UninstallProgress<'_>,
) -> anyhow::Result<()> {
    let UninstallProgress {
        agent_tools,
        hook_names,
        agent_hook_tools,
        root_gitignore_removed,
        syntext_gitignore_removed,
        export_gitignore_removed,
        project_data_deleted,
    } = progress;
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
    entry
        .agent_hooks
        .retain(|hook| !agent_hook_tools.iter().any(|tool| tool == &hook.tool));
    if root_gitignore_removed {
        entry.root_gitignore_entry_added = false;
    }
    if syntext_gitignore_removed {
        entry.syntext_gitignore_entry_added = false;
    }
    if export_gitignore_removed {
        entry.export_gitignore_entry_added = false;
        entry.export_gitignore_entry = None;
    }

    if project_data_deleted
        && entry.agents.is_empty()
        && entry.hooks.is_empty()
        && entry.agent_hooks.is_empty()
        && !entry.root_gitignore_entry_added
        && !entry.syntext_gitignore_entry_added
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
