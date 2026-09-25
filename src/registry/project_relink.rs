use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::{io, project_meta::resolve_project_index, registry_path, ProjectEntry, SCHEMA_VERSION};

const IDENTITY_FILE: &str = "project-identity.json";

#[derive(Deserialize, Serialize)]
struct ProjectIdentity {
    id: String,
}

/// Result of moving a managed project to a new repository path.
#[derive(Debug)]
pub struct RelinkOutcome {
    /// Updated registry row.
    pub entry: ProjectEntry,
    /// Previous project path.
    pub old_path: PathBuf,
    /// Install records whose absolute paths moved with the repository.
    pub updated_records: usize,
    /// Owned agent MCP entries repaired through agent-config.
    pub repaired_integrations: usize,
    /// External integration files containing old path references for manual review.
    pub manual_repairs: Vec<PathBuf>,
}

fn identity_path(root: &Path) -> PathBuf {
    root.join(".synrepo").join("state").join(IDENTITY_FILE)
}

fn read_identity(root: &Path) -> anyhow::Result<Option<String>> {
    let path = identity_path(root);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(
            serde_json::from_str::<ProjectIdentity>(&text)
                .with_context(|| format!("invalid project identity at {}", path.display()))?
                .id,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

pub(super) fn write_identity_if_initialized(entry: &ProjectEntry) -> anyhow::Result<()> {
    if !entry.path.join(".synrepo").is_dir() {
        return Ok(());
    }
    let path = identity_path(&entry.path);
    if let Some(existing) = read_identity(&entry.path)? {
        anyhow::ensure!(
            existing == entry.effective_id(),
            "project identity at {} belongs to {existing}",
            path.display()
        );
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().expect("identity has parent"))?;
    let json = serde_json::to_vec(&ProjectIdentity {
        id: entry.effective_id(),
    })?;
    crate::util::atomic_write(&path, &json)
        .with_context(|| format!("writing project identity at {}", path.display()))
}

/// Move one registry identity to an initialized repository at a new path.
/// No repository data is deleted, and a matching marker is required when one exists.
pub fn relink_project(selector: &str, new_path: &Path) -> anyhow::Result<RelinkOutcome> {
    let path = registry_path().context("cannot write registry: no home directory detected")?;
    let mut registry = io::load_from(&path)?;
    let selector_path = Path::new(selector);
    let canonical_selector = (selector_path.is_absolute() || selector_path.exists())
        .then(|| super::canonicalize_path(selector_path));
    let index = resolve_project_index(&registry, selector, canonical_selector.as_deref())?;
    let target = std::fs::canonicalize(new_path)
        .with_context(|| format!("relink target does not exist: {}", new_path.display()))?;
    anyhow::ensure!(target.is_dir(), "relink target is not a directory");
    anyhow::ensure!(
        target.join(".synrepo").join("config.toml").is_file(),
        "relink target is not initialized: {}",
        target.display()
    );
    anyhow::ensure!(
        !registry.projects.iter().any(|entry| entry.path == target),
        "relink target is already managed: {}",
        target.display()
    );
    let mut entry = registry.projects[index].clone();
    let old_path = entry.path.clone();
    anyhow::ensure!(old_path != target, "project already uses that path");
    let id = entry.effective_id();
    if let Some(existing) = read_identity(&target)? {
        anyhow::ensure!(
            existing == id,
            "relink target belongs to project {existing}"
        );
    }
    entry.id = id;
    entry.path = target;
    let updated_records = rewrite_install_record_paths(&mut entry, &old_path);
    write_identity_if_initialized(&entry)?;
    registry.projects[index] = entry.clone();
    registry.schema_version = SCHEMA_VERSION;
    io::save_to(&path, &registry)?;
    let (repaired_integrations, manual_repairs) = repair_owned_integrations(&entry, &old_path);
    Ok(RelinkOutcome {
        entry,
        old_path,
        updated_records,
        repaired_integrations,
        manual_repairs,
    })
}

fn repair_owned_integrations(entry: &ProjectEntry, old_path: &Path) -> (usize, Vec<PathBuf>) {
    use agent_config::{InstallStatus, McpSpec, Scope};

    let mut repaired = 0;
    let mut manual = Vec::new();
    for agent in &entry.agents {
        let Some(config_path) = &agent.mcp_config_path else {
            continue;
        };
        let path = PathBuf::from(config_path);
        let path = if path.is_absolute() {
            path
        } else {
            entry.path.join(path)
        };
        let old_text = old_path.to_string_lossy();
        let contains_old = std::fs::read_to_string(&path)
            .ok()
            .is_some_and(|text| text.contains(old_text.as_ref()));
        if !contains_old {
            continue;
        }
        let scope = if agent.scope == "global" {
            Scope::Global
        } else {
            Scope::Local(entry.path.clone())
        };
        let Some(installer) = agent_config::mcp_by_id(&agent.tool) else {
            manual.push(path);
            continue;
        };
        let owned = installer
            .mcp_status(&scope, "synrepo", "synrepo")
            .is_ok_and(|status| matches!(status.status, InstallStatus::InstalledOwned { .. }));
        if !owned {
            manual.push(path);
            continue;
        }
        let args = vec![
            "mcp".to_string(),
            "--repo".to_string(),
            entry.path.display().to_string(),
        ];
        let spec = McpSpec::builder("synrepo")
            .owner("synrepo")
            .stdio("synrepo", args)
            .friendly_name("synrepo")
            .try_build();
        let applied = spec.is_ok_and(|spec| installer.install_mcp(&scope, &spec).is_ok());
        let still_old = std::fs::read_to_string(&path)
            .ok()
            .is_some_and(|text| text.contains(old_text.as_ref()));
        if applied && !still_old {
            repaired += 1;
        } else {
            manual.push(path);
        }
    }
    (repaired, manual)
}

fn rewrite_install_record_paths(entry: &mut ProjectEntry, old_path: &Path) -> usize {
    let mut updated = 0;
    let new_path = entry.path.clone();
    let mut rewrite = |value: &mut String| {
        let path = Path::new(value);
        if let Ok(relative) = path.strip_prefix(old_path) {
            *value = new_path.join(relative).to_string_lossy().into_owned();
            updated += 1;
        }
    };
    for agent in &mut entry.agents {
        rewrite(&mut agent.shim_path);
        if let Some(path) = &mut agent.mcp_config_path {
            rewrite(path);
        }
        if let Some(path) = &mut agent.mcp_backup_path {
            rewrite(path);
        }
    }
    for hook in &mut entry.hooks {
        rewrite(&mut hook.path);
    }
    for hook in &mut entry.agent_hooks {
        rewrite(&mut hook.path);
    }
    updated
}

/// Bounded sibling scan. Candidates are hints and are never relinked automatically.
pub fn relocation_candidates(entry: &ProjectEntry) -> Vec<PathBuf> {
    let Some(parent) = entry.path.parent() else {
        return Vec::new();
    };
    let Ok(children) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    children
        .take(512)
        .filter_map(Result::ok)
        .map(|child| child.path())
        .filter(|path| path != &entry.path && path.is_dir())
        .filter(|path| {
            read_identity(path)
                .ok()
                .flatten()
                .is_some_and(|id| id == entry.effective_id())
        })
        .collect()
}
