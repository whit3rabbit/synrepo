use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;
use synrepo::bootstrap::{bootstrap, probe, Missing, RuntimeClassification};
use synrepo::config::Config;
use synrepo::registry::{self, ProjectEntry};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ProjectHealthState {
    Ready,
    Missing,
    Unusable,
    Uninitialized,
    Partial,
}

impl ProjectHealthState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Missing => "missing",
            Self::Unusable => "unusable",
            Self::Uninitialized => "uninitialized",
            Self::Partial => "partial",
        }
    }
}

impl std::fmt::Display for ProjectHealthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct ProjectHealth {
    state: ProjectHealthState,
    detail: String,
}

impl ProjectHealth {
    fn ready() -> Self {
        Self {
            state: ProjectHealthState::Ready,
            detail: "ready".to_string(),
        }
    }

    fn is_ready(&self) -> bool {
        self.state == ProjectHealthState::Ready
    }
}

#[derive(Debug, Serialize)]
struct ProjectView {
    path: PathBuf,
    registry: ProjectEntry,
    health: ProjectHealth,
}

#[derive(Debug, Serialize)]
struct ProjectListJson {
    projects: Vec<ProjectView>,
}

#[derive(Debug, Serialize)]
struct ProjectInspectJson {
    managed: bool,
    path: PathBuf,
    registry: Option<ProjectEntry>,
    health: ProjectHealth,
    suggestion: Option<String>,
}

pub(crate) fn project_add(repo_root: &Path, path: Option<PathBuf>) -> anyhow::Result<()> {
    print!("{}", project_add_output(repo_root, path)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn project_add_output(
    repo_root: &Path,
    path: Option<PathBuf>,
) -> anyhow::Result<String> {
    let target = resolve_project_path(repo_root, path)?;
    if !target.exists() {
        anyhow::bail!("project add target does not exist: {}", target.display());
    }
    if !target.is_dir() {
        anyhow::bail!(
            "project add target is not a directory: {}",
            target.display()
        );
    }

    let already_managed = registry::contains_project(&target)?;
    if !Config::synrepo_dir(&target).exists() {
        bootstrap(&target, None, false)?;
    }

    let health = project_health(&target);
    if !health.is_ready() {
        anyhow::bail!(
            "project is not ready and was not registered: {} ({})",
            target.display(),
            health.detail
        );
    }

    let entry = registry::record_project(&target)?;
    let action = if already_managed {
        "already managed"
    } else {
        "registered"
    };
    Ok(format!(
        "Project {action}: {}\n  id: {}\n  name: {}\n  health: {}\n  initialized_at: {}\n",
        entry.path.display(),
        entry.effective_id(),
        entry.display_name(),
        health.state,
        entry.initialized_at
    ))
}

pub(crate) fn project_list(json: bool) -> anyhow::Result<()> {
    print!("{}", project_list_output(json)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn project_list_output(json: bool) -> anyhow::Result<String> {
    let registry = registry::load()?;
    let projects: Vec<ProjectView> = registry
        .projects
        .into_iter()
        .map(|entry| ProjectView {
            health: project_health(&entry.path),
            path: entry.path.clone(),
            registry: entry,
        })
        .collect();

    if json {
        return Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&ProjectListJson { projects })?
        ));
    }

    if projects.is_empty() {
        return Ok("No managed projects.\n".to_string());
    }

    let mut out = String::new();
    for project in projects {
        writeln!(
            out,
            "{}  {}  {} [{}] {}",
            project.registry.effective_id(),
            project.registry.display_name(),
            project.path.display(),
            project.health.state,
            project.health.detail
        )
        .unwrap();
    }
    Ok(out)
}

pub(crate) fn project_inspect(
    repo_root: &Path,
    path: Option<PathBuf>,
    json: bool,
) -> anyhow::Result<()> {
    print!("{}", project_inspect_output(repo_root, path, json)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn project_inspect_output(
    repo_root: &Path,
    path: Option<PathBuf>,
    json: bool,
) -> anyhow::Result<String> {
    let target = resolve_project_path(repo_root, path)?;
    let entry = registry::get(&target)?;
    let health = project_health(&target);
    let suggestion = entry
        .is_none()
        .then(|| format!("synrepo project add {}", target.display()));

    if json {
        return Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&ProjectInspectJson {
                managed: entry.is_some(),
                path: target,
                registry: entry,
                health,
                suggestion,
            })?
        ));
    }

    match entry {
        Some(entry) => Ok(format!(
            "Managed project: {}\n  id: {}\n  name: {}\n  health: {} ({})\n  initialized_at: {}\n",
            entry.path.display(),
            entry.effective_id(),
            entry.display_name(),
            health.state,
            health.detail,
            entry.initialized_at
        )),
        None => Ok(format!(
            "Unmanaged project: {}\n  health: {} ({})\n  Next: {}\n",
            target.display(),
            health.state,
            health.detail,
            suggestion.expect("unmanaged project has suggestion")
        )),
    }
}

pub(crate) fn project_use(selector: &str) -> anyhow::Result<()> {
    print!("{}", project_use_output(selector)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn project_use_output(selector: &str) -> anyhow::Result<String> {
    let entry = registry::mark_project_opened(selector)?;
    Ok(format!(
        "Project selected: {}\n  id: {}\n  name: {}\n  path: {}\n",
        entry.display_name(),
        entry.effective_id(),
        entry.display_name(),
        entry.path.display()
    ))
}

pub(crate) fn project_rename(selector: &str, name: &str) -> anyhow::Result<()> {
    print!("{}", project_rename_output(selector, name)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn project_rename_output(selector: &str, name: &str) -> anyhow::Result<String> {
    let entry = registry::rename_project(selector, name)?;
    Ok(format!(
        "Project renamed: {}\n  id: {}\n  path: {}\n",
        entry.display_name(),
        entry.effective_id(),
        entry.path.display()
    ))
}

pub(crate) fn project_remove(repo_root: &Path, path: Option<PathBuf>) -> anyhow::Result<()> {
    print!("{}", project_remove_output(repo_root, path)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn project_remove_output(
    repo_root: &Path,
    path: Option<PathBuf>,
) -> anyhow::Result<String> {
    let target = resolve_project_path(repo_root, path)?;
    match registry::remove_project(&target)? {
        Some(entry) => Ok(format!(
            "Project unmanaged: {}\n  repository state left untouched\n",
            entry.path.display()
        )),
        None => Ok(format!(
            "No managed project found for: {}\n  repository state left untouched\n",
            target.display()
        )),
    }
}

fn resolve_project_path(repo_root: &Path, path: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let target = path.unwrap_or_else(|| repo_root.to_path_buf());
    Ok(registry::canonicalize_path(&target))
}

pub(super) fn project_health(path: &Path) -> ProjectHealth {
    if !path.exists() {
        return ProjectHealth {
            state: ProjectHealthState::Missing,
            detail: "path does not exist".to_string(),
        };
    }
    if !path.is_dir() {
        return ProjectHealth {
            state: ProjectHealthState::Unusable,
            detail: "path is not a directory".to_string(),
        };
    }

    match probe(path).classification {
        RuntimeClassification::Ready => ProjectHealth::ready(),
        RuntimeClassification::Uninitialized => ProjectHealth {
            state: ProjectHealthState::Uninitialized,
            detail: ".synrepo/ is missing".to_string(),
        },
        RuntimeClassification::Partial { missing } => ProjectHealth {
            state: ProjectHealthState::Partial,
            detail: missing_summary(&missing),
        },
    }
}

fn missing_summary(missing: &[Missing]) -> String {
    if missing.is_empty() {
        return "unknown partial state".to_string();
    }
    missing
        .iter()
        .map(missing_detail)
        .collect::<Vec<_>>()
        .join("; ")
}

fn missing_detail(missing: &Missing) -> String {
    match missing {
        Missing::ConfigFile => "config file missing".to_string(),
        Missing::ConfigUnreadable { detail } => format!("config unreadable: {detail}"),
        Missing::GraphStore => "graph store missing or unreadable".to_string(),
        Missing::CompatBlocked { guidance } => {
            if guidance.is_empty() {
                "compatibility blocked".to_string()
            } else {
                format!("compatibility blocked: {}", guidance.join("; "))
            }
        }
        Missing::CompatEvaluationFailed { detail } => {
            format!("compatibility evaluation failed: {detail}")
        }
    }
}
