//! Binary uninstall detection.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum BinaryTeardown {
    DeleteDirect {
        path: PathBuf,
    },
    ManualCommand {
        command: String,
        reason: String,
    },
    Skipped {
        path: Option<PathBuf>,
        reason: String,
    },
}

pub(crate) fn detect(repo_root: &Path, keep_binary: bool) -> BinaryTeardown {
    if keep_binary {
        return BinaryTeardown::Skipped {
            path: current_exe().ok(),
            reason: "--keep-binary was passed".to_string(),
        };
    }
    match current_exe() {
        Ok(path) => classify(repo_root, &path),
        Err(err) => BinaryTeardown::Skipped {
            path: None,
            reason: format!("could not resolve current executable: {err}"),
        },
    }
}

fn current_exe() -> std::io::Result<PathBuf> {
    std::env::current_exe()
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn classify(repo_root: &Path, path: &Path) -> BinaryTeardown {
    if is_repo_or_build_binary(repo_root, path) {
        return BinaryTeardown::Skipped {
            path: Some(path.to_path_buf()),
            reason: "binary appears to be the workspace build output".to_string(),
        };
    }

    let text = path.to_string_lossy();
    if text.ends_with("/.cargo/bin/synrepo") || text.ends_with("\\.cargo\\bin\\synrepo.exe") {
        return BinaryTeardown::ManualCommand {
            command: "cargo uninstall synrepo".to_string(),
            reason: "binary appears to be managed by Cargo".to_string(),
        };
    }
    if text == "/opt/homebrew/bin/synrepo" || text == "/usr/local/bin/synrepo" {
        return BinaryTeardown::ManualCommand {
            command: "brew uninstall --cask synrepo".to_string(),
            reason: "binary appears to be managed by Homebrew".to_string(),
        };
    }

    #[cfg(windows)]
    {
        return BinaryTeardown::ManualCommand {
            command: format!("Remove-Item -LiteralPath '{}' -Force", path.display()),
            reason: "Windows cannot reliably delete the running executable".to_string(),
        };
    }

    #[cfg(not(windows))]
    {
        if is_direct_install_path(path) {
            return BinaryTeardown::DeleteDirect {
                path: path.to_path_buf(),
            };
        }
    }

    BinaryTeardown::Skipped {
        path: Some(path.to_path_buf()),
        reason: "binary install method is unknown; remove it manually after reviewing the path"
            .to_string(),
    }
}

fn is_repo_or_build_binary(repo_root: &Path, path: &Path) -> bool {
    if path.starts_with(repo_root) {
        return true;
    }
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .map(|part| part == "target")
            .unwrap_or(false)
    })
}

#[cfg(not(windows))]
fn is_direct_install_path(path: &Path) -> bool {
    let Some(home) = synrepo::config::home_dir() else {
        return false;
    };
    path == home.join(".local/bin/synrepo")
}

#[cfg(test)]
mod tests {
    use super::{classify, BinaryTeardown};

    #[test]
    fn cargo_binary_uses_manual_cargo_uninstall() {
        let home = std::path::Path::new("/tmp/home");
        let out = classify(
            std::path::Path::new("/repo"),
            &home.join(".cargo/bin/synrepo"),
        );
        assert!(matches!(
            out,
            BinaryTeardown::ManualCommand { ref command, .. } if command == "cargo uninstall synrepo"
        ));
    }

    #[test]
    fn homebrew_binary_uses_manual_brew_uninstall() {
        let out = classify(
            std::path::Path::new("/repo"),
            std::path::Path::new("/opt/homebrew/bin/synrepo"),
        );
        assert!(matches!(
            out,
            BinaryTeardown::ManualCommand { ref command, .. } if command == "brew uninstall --cask synrepo"
        ));
    }

    #[test]
    fn target_binary_is_never_deleted() {
        let out = classify(
            std::path::Path::new("/repo"),
            std::path::Path::new("/repo/target/debug/synrepo"),
        );
        assert!(matches!(out, BinaryTeardown::Skipped { .. }));
    }

    #[cfg(not(windows))]
    #[test]
    fn direct_local_binary_is_deleted_when_home_matches() {
        let _lock =
            synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
        let home = tempfile::tempdir().unwrap();
        let _guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let out = classify(
            std::path::Path::new("/repo"),
            &home.path().join(".local/bin/synrepo"),
        );
        assert!(matches!(out, BinaryTeardown::DeleteDirect { .. }));
    }
}
