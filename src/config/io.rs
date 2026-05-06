use std::path::{Path, PathBuf};

pub(super) fn reject_legacy_explain_block(text: &str, path: &Path) -> crate::Result<()> {
    let Ok(value) = toml::from_str::<toml::Value>(text) else {
        return Ok(());
    };
    if value.get("synthesis").is_some() {
        return Err(crate::Error::Config(format!(
            "{} uses legacy [synthesis]; rename it to [explain]",
            path.display()
        )));
    }
    Ok(())
}

/// Best-effort home-directory resolver: `$HOME` on Unix, `%USERPROFILE%` on
/// Windows, `None` on bare/unsupported targets.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}
