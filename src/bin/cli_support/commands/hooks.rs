//! Git hook install and uninstall helpers.

use std::path::{Path, PathBuf};

use synrepo::pipeline::writer::now_rfc3339;
use synrepo::registry::HookEntry;

pub(crate) const HOOK_NAMES: &[&str] = &["post-commit", "post-merge", "post-checkout"];
pub(crate) const HOOK_BEGIN: &str = "# >>> synrepo hook >>>";
pub(crate) const HOOK_END: &str = "# <<< synrepo hook <<<";
const HOOK_COMMAND: &str = "(synrepo reconcile --fast > /dev/null 2>&1 &)";

/// Install Git hooks (post-commit, post-merge, post-checkout) to trigger reconcile --fast.
pub(crate) fn install_hooks(repo_root: &Path) -> anyhow::Result<()> {
    let repo = synrepo::pipeline::git::open_repo(repo_root)
        .map_err(|e| anyhow::anyhow!("install-hooks: not a git repository: {e}"))?;

    let hooks_dir = repo.git_dir().join("hooks");
    if !hooks_dir.exists() {
        std::fs::create_dir_all(&hooks_dir)
            .map_err(|e| anyhow::anyhow!("could not create hooks directory: {e}"))?;
    }

    let mut records = Vec::new();
    for hook_name in HOOK_NAMES {
        let hook_path = hooks_dir.join(hook_name);
        let mode = install_one_hook(&hook_path, hook_name)?;
        records.push(HookEntry {
            name: (*hook_name).to_string(),
            path: registry_path_string(repo_root, &hook_path),
            mode,
            installed_at: now_rfc3339(),
        });
    }

    if let Err(err) = synrepo::registry::record_hooks(repo_root, records) {
        tracing::warn!(error = %err, "install registry update skipped after hook install");
    }

    println!(
        "Git hooks installed successfully in {}.",
        hooks_dir.display()
    );
    Ok(())
}

fn install_one_hook(hook_path: &Path, hook_name: &str) -> anyhow::Result<String> {
    if hook_path.exists() {
        let existing = std::fs::read_to_string(hook_path)?;
        if existing.contains(HOOK_BEGIN) && existing.contains(HOOK_END) {
            println!("  hook already installed: {hook_name}");
            return Ok("marked_block".to_string());
        }
        if existing.contains("synrepo reconcile") {
            println!("  legacy synrepo hook already installed: {hook_name}");
            return Ok("legacy".to_string());
        }
        append_marked_hook(hook_path)?;
        println!("  appended to existing hook: {hook_name}");
        return Ok("marked_block".to_string());
    }

    std::fs::write(hook_path, full_hook_script())?;
    make_executable(hook_path)?;
    println!("  installed new hook: {hook_name}");
    Ok("full_file".to_string())
}

fn append_marked_hook(hook_path: &Path) -> anyhow::Result<()> {
    use std::io::Write;

    let mut f = std::fs::OpenOptions::new().append(true).open(hook_path)?;
    writeln!(f, "\n{}", marked_hook_block())?;
    Ok(())
}

pub(crate) fn full_hook_script() -> String {
    format!(
        "#!/bin/bash\n# synrepo hook: keep the graph aligned with the working tree.\n{}\n",
        marked_hook_block()
    )
}

pub(crate) fn marked_hook_block() -> String {
    format!(
        "{HOOK_BEGIN}\n# Run reconcile in the background so it doesn't block the git command.\n{HOOK_COMMAND}\n{HOOK_END}"
    )
}

#[cfg(unix)]
fn make_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

pub(crate) fn registry_path_string(repo_root: &Path, path: &Path) -> String {
    match path.strip_prefix(repo_root).map(PathBuf::from) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use tempfile::tempdir;

    use super::*;

    fn isolated_home() -> (
        tempfile::TempDir,
        synrepo::config::test_home::HomeEnvGuard,
        synrepo::test_support::GlobalTestLock,
    ) {
        let lock =
            synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
        let home = tempdir().unwrap();
        let guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
        (home, guard, lock)
    }

    fn init_git_repo(path: &Path) {
        let output = Command::new("git")
            .arg("init")
            .arg(path)
            .output()
            .expect("git init should run");
        assert!(
            output.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn new_hook_writes_marked_full_file() {
        let hook_dir = tempdir().unwrap();
        let hook_path = hook_dir.path().join("post-commit");

        let mode = install_one_hook(&hook_path, "post-commit").unwrap();

        assert_eq!(mode, "full_file");
        let raw = fs::read_to_string(&hook_path).unwrap();
        assert!(raw.contains(HOOK_BEGIN));
        assert!(raw.contains(HOOK_END));
        assert!(raw.contains("synrepo reconcile --fast"));
    }

    #[test]
    fn existing_user_hook_gets_marked_block() {
        let hook_dir = tempdir().unwrap();
        let hook_path = hook_dir.path().join("post-merge");
        fs::write(&hook_path, "#!/bin/sh\necho user\n").unwrap();

        let mode = install_one_hook(&hook_path, "post-merge").unwrap();

        assert_eq!(mode, "marked_block");
        let raw = fs::read_to_string(&hook_path).unwrap();
        assert!(raw.contains("echo user"));
        assert!(raw.contains(HOOK_BEGIN));
        assert!(raw.contains(HOOK_END));
    }

    #[test]
    fn install_hooks_records_all_hook_entries() {
        let (_home, _guard, _lock) = isolated_home();
        let project = tempdir().unwrap();
        init_git_repo(project.path());

        install_hooks(project.path()).unwrap();

        let entry = synrepo::registry::get(project.path())
            .unwrap()
            .expect("project hook entry should be recorded");
        assert_eq!(entry.hooks.len(), HOOK_NAMES.len());
        for hook_name in HOOK_NAMES {
            let hook = entry
                .hooks
                .iter()
                .find(|hook| hook.name == *hook_name)
                .expect("hook record should exist");
            assert_eq!(hook.mode, "full_file");
            assert_eq!(hook.path, format!(".git/hooks/{hook_name}"));
        }
    }
}
