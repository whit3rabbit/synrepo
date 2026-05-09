use std::path::Path;

use crate::util::atomic_write;

pub(super) fn write_synrepo_gitignore(synrepo_dir: &Path) -> anyhow::Result<()> {
    let gitignore_path = synrepo_dir.join(".gitignore");
    atomic_write_file(
        &gitignore_path,
        b"# Gitignore everything in .synrepo/\n\
         *\n\
         !.gitignore\n\
         # Generated vectors directory (semantic-triage)\n\
         index/vectors/\n",
    )?;
    Ok(())
}

/// Return true when the repository root `.gitignore` contains `.synrepo/`.
pub fn root_gitignore_contains_synrepo(repo_root: &Path) -> anyhow::Result<bool> {
    root_gitignore_contains_line(repo_root, ".synrepo/")
}

/// Append `.synrepo/` to the repository root `.gitignore` if needed.
///
/// Returns true only when this call wrote the line, so callers can record
/// synrepo ownership for uninstall/remove cleanup.
pub fn ensure_root_gitignore_entry(repo_root: &Path) -> anyhow::Result<bool> {
    let gitignore_path = repo_root.join(".gitignore");
    let entry = ".synrepo/";

    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;
        if content.lines().any(|line| line.trim() == entry) {
            return Ok(false);
        }
        let mut new_content = content;
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(entry);
        new_content.push('\n');
        std::fs::write(&gitignore_path, new_content)?;
    } else {
        std::fs::write(&gitignore_path, format!("{entry}\n"))?;
    }
    Ok(true)
}

/// Remove a line from the repository root `.gitignore`.
///
/// Callers should only remove lines the install registry records as
/// synrepo-owned. User-owned ignore entries must be preserved.
pub fn remove_from_root_gitignore(repo_root: &Path, entry: &str) -> anyhow::Result<bool> {
    let gitignore_path = repo_root.join(".gitignore");
    if !gitignore_path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(&gitignore_path)?;
    let had_trailing_newline = content.ends_with('\n');
    let mut removed = false;
    let mut kept = Vec::with_capacity(content.lines().count());
    for line in content.lines() {
        if !removed && line.trim() == entry {
            removed = true;
            continue;
        }
        kept.push(line);
    }
    if !removed {
        return Ok(false);
    }
    let mut new_content = kept.join("\n");
    if had_trailing_newline && !new_content.is_empty() {
        new_content.push('\n');
    }
    std::fs::write(&gitignore_path, new_content)?;
    Ok(true)
}

fn root_gitignore_contains_line(repo_root: &Path, entry: &str) -> anyhow::Result<bool> {
    let gitignore_path = repo_root.join(".gitignore");
    if !gitignore_path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(gitignore_path)?;
    Ok(content.lines().any(|line| line.trim() == entry))
}

fn atomic_write_file(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(path, contents)?;
    Ok(())
}
