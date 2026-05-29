use serde::{Deserialize, Serialize};

/// Branch refs that synrepo should index as read-only snapshot roots.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BranchRootsConfig {
    /// Full local ref names, e.g. `refs/heads/main`.
    #[serde(default)]
    pub refs: Vec<String>,
    /// Watch polling interval for configured local refs.
    #[serde(default = "default_branch_poll_seconds")]
    pub poll_seconds: u32,
}

impl Default for BranchRootsConfig {
    fn default() -> Self {
        Self {
            refs: Vec::new(),
            poll_seconds: default_branch_poll_seconds(),
        }
    }
}

impl BranchRootsConfig {
    pub(crate) fn validate(&self) -> crate::Result<()> {
        for ref_name in &self.refs {
            validate_exact_ref(ref_name)?;
        }
        Ok(())
    }
}

pub(crate) fn default_branch_poll_seconds() -> u32 {
    30
}

fn validate_exact_ref(ref_name: &str) -> crate::Result<()> {
    let valid_prefix = ref_name.starts_with("refs/heads/") || ref_name.starts_with("refs/remotes/");
    let suffix = ref_name
        .strip_prefix("refs/heads/")
        .or_else(|| ref_name.strip_prefix("refs/remotes/"))
        .unwrap_or("");
    let invalid = ref_name.is_empty()
        || !valid_prefix
        || suffix.is_empty()
        || (ref_name.starts_with("refs/remotes/") && !suffix.contains('/'))
        || suffix.starts_with('/')
        || suffix.ends_with('/')
        || ref_name.contains("..")
        || ref_name.contains("@{")
        || ref_name.contains("//")
        || ref_name.chars().any(invalid_ref_char);

    if invalid {
        return Err(crate::Error::Config(format!(
            "branch_roots.refs entries must be exact local refs under refs/heads/ or refs/remotes/: `{ref_name}`"
        )));
    }
    Ok(())
}

fn invalid_ref_char(ch: char) -> bool {
    ch.is_ascii_control()
        || ch.is_whitespace()
        || matches!(ch, '\\' | '~' | '^' | ':' | '?' | '*' | '[')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_disable_branch_roots() {
        let config = BranchRootsConfig::default();
        assert!(config.refs.is_empty());
        assert_eq!(config.poll_seconds, 30);
    }

    #[test]
    fn validation_accepts_full_local_refs_only() {
        let valid = BranchRootsConfig {
            refs: vec![
                "refs/heads/main".to_string(),
                "refs/remotes/origin/feature".to_string(),
            ],
            ..BranchRootsConfig::default()
        };
        valid.validate().unwrap();

        for bad in [
            "main",
            "refs/tags/v1",
            "refs/heads/",
            "refs/heads/a..b",
            "refs/heads/a b",
            "refs/remotes/origin",
            "refs/remotes/origin//feature",
            "refs/heads/feature@{1}",
        ] {
            let config = BranchRootsConfig {
                refs: vec![bad.to_string()],
                ..BranchRootsConfig::default()
            };
            assert!(config.validate().is_err(), "{bad}");
        }
    }
}
