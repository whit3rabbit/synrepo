use std::path::PathBuf;

use agent_config::Scope;

use crate::cli_support::agent_shims::AgentTool;

use super::CodexSkillWarning;

pub(super) fn codex_global_skill_warnings() -> Vec<CodexSkillWarning> {
    let mut paths = Vec::new();
    if let Some(path) = AgentTool::Codex.resolved_shim_output_path(&Scope::Global) {
        paths.push(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(
            PathBuf::from(home)
                .join(".agents")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
        );
    }
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .filter_map(codex_skill_warning_for_path)
        .collect()
}

fn codex_skill_warning_for_path(path: PathBuf) -> Option<CodexSkillWarning> {
    let existing = std::fs::read_to_string(&path).ok()?;
    codex_skill_warning_for_content(&existing).map(|(content_differs, duplicate_frontmatter)| {
        CodexSkillWarning {
            path,
            content_differs,
            duplicate_frontmatter,
        }
    })
}

pub(super) fn codex_skill_warning_for_content(content: &str) -> Option<(bool, bool)> {
    let content_differs = content != AgentTool::Codex.shim_content();
    let duplicate_frontmatter = has_duplicate_frontmatter(content);
    if content_differs || duplicate_frontmatter {
        Some((content_differs, duplicate_frontmatter))
    } else {
        None
    }
}

fn has_duplicate_frontmatter(content: &str) -> bool {
    content
        .lines()
        .filter(|line| line.trim() == "---")
        .take(4)
        .count()
        >= 4
}

pub(super) fn codex_skill_warning_reason(warning: &CodexSkillWarning) -> &'static str {
    match (warning.content_differs, warning.duplicate_frontmatter) {
        (true, true) => "differs from the generated shim and has duplicate frontmatter",
        (true, false) => "differs from the generated shim",
        (false, true) => "has duplicate frontmatter",
        (false, false) => "needs review",
    }
}
