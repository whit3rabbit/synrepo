use std::fmt::Write as _;
use std::path::PathBuf;

use serde::Serialize;
use synrepo::registry::{self, ProjectEntry};

use super::project::{project_health, ProjectHealth};

#[derive(Debug, Serialize)]
struct MissingProject {
    path: PathBuf,
    registry: ProjectEntry,
    health: ProjectHealth,
    detail: &'static str,
}

#[derive(Debug, Serialize)]
struct ProjectPruneMissingJson {
    applied: bool,
    missing_count: usize,
    missing_projects: Vec<MissingProject>,
}

pub(crate) fn project_prune_missing(apply: bool, json: bool) -> anyhow::Result<()> {
    print!("{}", project_prune_missing_output(apply, json)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn project_prune_missing_output(apply: bool, json: bool) -> anyhow::Result<String> {
    let missing_projects = missing_projects()?;

    if apply {
        for project in &missing_projects {
            registry::remove_project(&project.path)?;
        }
    }

    if json {
        return Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&ProjectPruneMissingJson {
                applied: apply,
                missing_count: missing_projects.len(),
                missing_projects,
            })?
        ));
    }

    if missing_projects.is_empty() {
        return Ok("No missing managed projects.\n".to_string());
    }

    let mut out = String::new();
    let action = if apply {
        "Pruned missing managed projects"
    } else {
        "Missing managed projects"
    };
    writeln!(out, "{action} ({}):", missing_projects.len()).unwrap();
    for project in &missing_projects {
        writeln!(
            out,
            "  {}  {}  {}",
            project.registry.effective_id(),
            project.registry.display_name(),
            project.path.display()
        )
        .unwrap();
    }
    if apply {
        writeln!(
            out,
            "No repository state, .synrepo/, global config, or agent files were deleted."
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "Dry run: rerun with `synrepo project prune-missing --apply` to unregister these entries."
        )
        .unwrap();
    }
    Ok(out)
}

fn missing_projects() -> anyhow::Result<Vec<MissingProject>> {
    Ok(registry::load()?
        .projects
        .into_iter()
        .filter(|entry| !entry.path.exists())
        .map(|entry| MissingProject {
            path: entry.path.clone(),
            health: project_health(&entry.path),
            registry: entry,
            detail: "path does not exist",
        })
        .collect())
}
