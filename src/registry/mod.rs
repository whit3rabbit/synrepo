//! User-level install registry at `~/.synrepo/projects.toml`.
//!
//! Records what artifacts synrepo has installed per project so `synrepo remove`
//! knows exactly what to undo. Missing entries are non-fatal: the removal path
//! falls back to scanning every `AgentTool::output_path` and every known MCP
//! config file. Registry-driven removal is the fast path; filesystem scanning
//! is the correctness floor for pre-existing installs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::home_dir;
use crate::pipeline::writer::now_rfc3339;

pub mod io;

#[cfg(test)]
mod tests;

/// Current schema version. Bump on any breaking change to the on-disk shape.
pub const SCHEMA_VERSION: u32 = 1;

/// Root document of `~/.synrepo/projects.toml`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Registry {
    /// Schema version for forward-compat reads.
    #[serde(default)]
    pub schema_version: u32,
    /// One entry per project this user has initialized.
    #[serde(default, rename = "project")]
    pub projects: Vec<ProjectEntry>,
}

/// One project's install record.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProjectEntry {
    /// Canonicalized absolute path to the repo root.
    pub path: PathBuf,
    /// ISO 8601 UTC timestamp of the first-seen install.
    pub initialized_at: String,
    /// Relative path to the synrepo dir inside the project. Always ".synrepo"
    /// today; stored so a future per-project override is representable.
    #[serde(default = "default_synrepo_dir")]
    pub synrepo_dir: String,
    /// True iff synrepo appended `.synrepo/` to the project's root .gitignore.
    /// Removal only strips the line when this is true, so a user-authored line
    /// is never touched.
    #[serde(default)]
    pub root_gitignore_entry_added: bool,
    /// True iff `synrepo export` appended `synrepo-context/` to the root
    /// .gitignore. Same removal rule as above.
    #[serde(default)]
    pub export_gitignore_entry_added: bool,
    /// Per-agent install records (one per successful `setup` / `agent-setup`).
    #[serde(default, rename = "agents")]
    pub agents: Vec<AgentEntry>,
}

/// One per-agent install record inside a [`ProjectEntry`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentEntry {
    /// Matches `AgentTool::as_str()` / the CLI value (e.g. "claude", "codex").
    pub tool: String,
    /// Install scope for this agent entry: "project" or "global".
    #[serde(default = "default_agent_scope")]
    pub scope: String,
    /// Path of the shim file. Project-local paths are stored relative to the
    /// project root; global paths are stored absolute.
    pub shim_path: String,
    /// Path of the MCP config file we edited. Project-local paths are stored
    /// relative to the project root; global paths are stored absolute.
    /// `None` for shim-only-tier tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_config_path: Option<String>,
    /// Path of the pristine-state `.bak` sidecar for the MCP config, relative
    /// to the project root. Populated whenever a `.bak` exists after install
    /// (freshly created or pre-existing from a prior install). `None` when the
    /// user declined the backup prompt, the config didn't pre-exist, setup ran
    /// non-interactively, or the tool has no automated MCP config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_backup_path: Option<String>,
    /// ISO 8601 UTC timestamp of the latest install for this tool.
    pub installed_at: String,
}

fn default_synrepo_dir() -> String {
    ".synrepo".to_string()
}

fn default_agent_scope() -> String {
    "project".to_string()
}

/// Return the default registry path (`~/.synrepo/projects.toml`).
///
/// Returns `None` when the process has no detectable home directory (rare on
/// Unix/Windows, possible in bare containers). Callers should treat this as
/// "registry disabled" and fall back to filesystem scanning.
pub fn registry_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".synrepo").join("projects.toml"))
}

/// Load the registry from the default path (`~/.synrepo/projects.toml`).
///
/// Returns an empty [`Registry`] if the file does not exist, is empty, or the
/// home directory cannot be resolved. Surfaces an error only when the file
/// exists but fails to parse, so a corrupt registry is loud rather than silent.
pub fn load() -> anyhow::Result<Registry> {
    match registry_path() {
        Some(p) => io::load_from(&p),
        None => Ok(Registry::default()),
    }
}

/// Persist `registry` to the default path atomically.
pub fn save(registry: &Registry) -> anyhow::Result<()> {
    match registry_path() {
        Some(p) => io::save_to(&p, registry),
        None => Err(anyhow::anyhow!(
            "cannot write registry: no home directory detected"
        )),
    }
}

/// Fetch the entry for a given project path, if any.
///
/// The comparison canonicalizes `project` and every stored path, so symlink
/// aliases of the same repo resolve to the same entry.
pub fn get(project: &Path) -> anyhow::Result<Option<ProjectEntry>> {
    let registry = load()?;
    Ok(find_project(&registry, project).cloned())
}

/// Return true when `project` is present in the user-level registry.
pub fn contains_project(project: &Path) -> anyhow::Result<bool> {
    let registry = load()?;
    Ok(find_project(&registry, project).is_some())
}

/// Record a managed project without changing existing per-agent metadata.
///
/// Preserves `initialized_at` and all install metadata on an existing entry.
/// This is the user-facing project-manager path; install/uninstall flows keep
/// using their narrower helpers so removal metadata semantics stay unchanged.
pub fn record_project(project: &Path) -> anyhow::Result<ProjectEntry> {
    let Some(path) = registry_path() else {
        anyhow::bail!("cannot write registry: no home directory detected");
    };
    let mut registry = io::load_from(&path)?;
    registry.schema_version = SCHEMA_VERSION;
    let canonical = canonicalize(project);
    if let Some(existing) = find_project_mut(&mut registry, &canonical) {
        let entry = existing.clone();
        io::save_to(&path, &registry)?;
        return Ok(entry);
    }

    let entry = ProjectEntry {
        path: canonical,
        initialized_at: now_rfc3339(),
        synrepo_dir: default_synrepo_dir(),
        root_gitignore_entry_added: false,
        export_gitignore_entry_added: false,
        agents: Vec::new(),
    };
    registry.projects.push(entry.clone());
    io::save_to(&path, &registry)?;
    Ok(entry)
}

/// Record (or update) a project's init metadata.
///
/// Preserves `initialized_at` on an existing entry so the timestamp records
/// first-seen, not latest-seen. `root_gitignore_entry_added` is OR'd into the
/// existing value: once we've appended a line, the flag stays true until the
/// project is uninstalled.
pub fn record_install(project: &Path, root_gitignore_added: bool) -> anyhow::Result<()> {
    let Some(path) = registry_path() else {
        return Ok(());
    };
    let mut registry = io::load_from(&path)?;
    registry.schema_version = SCHEMA_VERSION;
    let canonical = canonicalize(project);
    match find_project_mut(&mut registry, &canonical) {
        Some(entry) => {
            entry.root_gitignore_entry_added |= root_gitignore_added;
        }
        None => {
            registry.projects.push(ProjectEntry {
                path: canonical,
                initialized_at: now_rfc3339(),
                synrepo_dir: default_synrepo_dir(),
                root_gitignore_entry_added: root_gitignore_added,
                export_gitignore_entry_added: false,
                agents: Vec::new(),
            });
        }
    }
    io::save_to(&path, &registry)
}

/// Record (or update) a per-agent install for the given project.
///
/// Creates the [`ProjectEntry`] if it doesn't exist yet (which can happen when
/// `synrepo setup` runs before an explicit `synrepo init` in some flows).
pub fn record_agent(project: &Path, agent: AgentEntry) -> anyhow::Result<()> {
    let Some(path) = registry_path() else {
        return Ok(());
    };
    let mut registry = io::load_from(&path)?;
    registry.schema_version = SCHEMA_VERSION;
    let canonical = canonicalize(project);
    let entry = match registry.projects.iter_mut().find(|p| p.path == canonical) {
        Some(e) => e,
        None => {
            registry.projects.push(ProjectEntry {
                path: canonical,
                initialized_at: now_rfc3339(),
                synrepo_dir: default_synrepo_dir(),
                root_gitignore_entry_added: false,
                export_gitignore_entry_added: false,
                agents: Vec::new(),
            });
            registry
                .projects
                .last_mut()
                .expect("just pushed an entry; vec is non-empty")
        }
    };
    match entry.agents.iter_mut().find(|a| a.tool == agent.tool) {
        Some(existing) => *existing = agent,
        None => entry.agents.push(agent),
    }
    io::save_to(&path, &registry)
}

/// Mark that `synrepo export` appended `synrepo-context/` to the root
/// .gitignore. No-op if the project is not tracked yet.
pub fn record_export_gitignore(project: &Path) -> anyhow::Result<()> {
    let Some(path) = registry_path() else {
        return Ok(());
    };
    let mut registry = io::load_from(&path)?;
    let canonical = canonicalize(project);
    if let Some(entry) = find_project_mut(&mut registry, &canonical) {
        entry.export_gitignore_entry_added = true;
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

/// Remove a managed project entry without touching repository-local files.
pub fn remove_project(project: &Path) -> anyhow::Result<Option<ProjectEntry>> {
    let Some(path) = registry_path() else {
        anyhow::bail!("cannot write registry: no home directory detected");
    };
    let mut registry = io::load_from(&path)?;
    let canonical = canonicalize(project);
    let removed = registry
        .projects
        .iter()
        .position(|p| p.path == canonical)
        .map(|idx| registry.projects.remove(idx));
    if removed.is_some() {
        io::save_to(&path, &registry)?;
    }
    Ok(removed)
}

/// Canonicalize a project path using the registry comparison rules.
pub fn canonicalize_path(project: &Path) -> PathBuf {
    canonicalize(project)
}

fn find_project<'a>(registry: &'a Registry, project: &Path) -> Option<&'a ProjectEntry> {
    let canonical = canonicalize(project);
    registry.projects.iter().find(|p| p.path == canonical)
}

fn find_project_mut<'a>(
    registry: &'a mut Registry,
    project: &Path,
) -> Option<&'a mut ProjectEntry> {
    let canonical = canonicalize(project);
    registry.projects.iter_mut().find(|p| p.path == canonical)
}

/// Best-effort canonicalization: uses `fs::canonicalize` when the path exists,
/// otherwise falls back to `absolute`. Returns the raw path if neither works,
/// so comparison still behaves deterministically in tests on nonexistent dirs.
fn canonicalize(p: &Path) -> PathBuf {
    if let Ok(c) = std::fs::canonicalize(p) {
        return c;
    }
    if let Ok(abs) = absolute(p) {
        return abs;
    }
    p.to_path_buf()
}

fn absolute(p: &Path) -> std::io::Result<PathBuf> {
    if p.is_absolute() {
        return Ok(p.to_path_buf());
    }
    let cwd = std::env::current_dir()?;
    Ok(cwd.join(p))
}
