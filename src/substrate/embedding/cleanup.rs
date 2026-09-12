//! Cleanup planning for profile-keyed vector index storage.
//!
//! Identifies the artifacts under `.synrepo/index/vectors/` that no longer
//! match the active config profile: stale profile subdirectories from a
//! previous `semantic_model` / `embedding_dim` / `semantic_vector_precision`
//! value, and the legacy flat `index.bin` written by index format v5 and
//! earlier. Deletion is the caller's decision; this module only plans.

use std::path::PathBuf;

use super::profile::VectorProfile;

/// One removable artifact under the vectors root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupCandidate {
    /// Path to the removable artifact.
    pub path: PathBuf,
    /// What kind of artifact this is.
    pub kind: CleanupKind,
}

/// Kind of removable vector-index artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupKind {
    /// A profile-keyed subdirectory that does not match the active profile.
    StaleProfileDir,
    /// The legacy flat `index/vectors/index.bin` written by index format v5
    /// and earlier; v6 load refuses that path, so the file is dead weight.
    LegacyFlatIndex,
}

impl CleanupKind {
    /// Stable label used in JSON output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StaleProfileDir => "stale_profile_dir",
            Self::LegacyFlatIndex => "legacy_flat_index",
        }
    }
}

/// Plan the artifacts `synrepo embeddings clean` may remove: every
/// profile-shaped subdirectory of the vectors root except the active
/// profile's, plus a legacy flat `index.bin` when present. Read-only.
///
/// Directories whose names do not look like synrepo profile directories are
/// never listed — the vectors root is synrepo-owned, but an unrecognized
/// name may be something the user stashed there deliberately.
pub fn plan_vector_cleanup(
    synrepo_dir: &std::path::Path,
    config: &crate::config::Config,
) -> std::io::Result<Vec<CleanupCandidate>> {
    let vectors_root = synrepo_dir.join("index/vectors");
    let active_dir = VectorProfile::for_config(config).relative_dir();
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&vectors_root) {
        Ok(entries) => entries,
        // No vectors directory yet: nothing to clean.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(err) => return Err(err),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_dir() {
            if is_profile_dir_name(&name) && name != active_dir {
                out.push(CleanupCandidate {
                    path,
                    kind: CleanupKind::StaleProfileDir,
                });
            }
        } else if name == "index.bin" {
            out.push(CleanupCandidate {
                path,
                kind: CleanupKind::LegacyFlatIndex,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Whether a directory name looks like a synrepo profile directory
/// (`<16 hex chars>-<label>`), matching [`super::profile::VectorProfile::
/// relative_dir`].
fn is_profile_dir_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 17 || bytes[16] != b'-' {
        return false;
    }
    bytes[..16].iter().all(u8::is_ascii_hexdigit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> crate::config::Config {
        crate::config::Config::default()
    }

    fn active_dir_name(config: &crate::config::Config) -> String {
        VectorProfile::for_config(config).relative_dir()
    }

    #[test]
    fn plan_lists_stale_profile_dirs_and_legacy_flat_index() {
        let dir = tempfile::tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        let config = config();
        let vectors_root = synrepo_dir.join("index/vectors");
        std::fs::create_dir_all(vectors_root.join(active_dir_name(&config))).unwrap();
        let stale = vectors_root.join("deadbeefdeadbeef-onnx-old-d384-float32");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(vectors_root.join("index.bin"), b"legacy v5").unwrap();

        let plan = plan_vector_cleanup(&synrepo_dir, &config).unwrap();
        let paths: Vec<_> = plan.iter().map(|c| c.path.clone()).collect();
        assert_eq!(plan.len(), 2, "expected stale dir + legacy flat index");
        assert!(paths.contains(&stale));
        assert!(paths.contains(&vectors_root.join("index.bin")));
        assert!(plan
            .iter()
            .all(|c| c.kind != CleanupKind::StaleProfileDir || c.path == stale));
    }

    #[test]
    fn plan_keeps_active_profile_and_non_profile_entries() {
        let dir = tempfile::tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        let config = config();
        let vectors_root = synrepo_dir.join("index/vectors");
        let active = vectors_root.join(active_dir_name(&config));
        std::fs::create_dir_all(&active).unwrap();
        std::fs::write(active.join("index.bin"), b"active").unwrap();
        // Not profile-shaped: must never be listed.
        std::fs::create_dir_all(vectors_root.join("user-notes")).unwrap();
        std::fs::write(vectors_root.join("README.txt"), b"keep").unwrap();

        let plan = plan_vector_cleanup(&synrepo_dir, &config).unwrap();
        assert!(plan.is_empty(), "unexpected candidates: {plan:?}");
    }

    #[test]
    fn plan_with_missing_vectors_root_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan_vector_cleanup(&dir.path().join(".synrepo"), &config()).unwrap();
        assert!(plan.is_empty());
    }

    #[test]
    fn is_profile_dir_name_requires_hex_prefix_and_dash() {
        assert!(is_profile_dir_name("0123456789abcdef-label"));
        assert!(is_profile_dir_name("0123456789ABCDEF-label"));
        assert!(!is_profile_dir_name("0123456789abcde-label"));
        assert!(!is_profile_dir_name("0123456789abcdeflabel"));
        assert!(!is_profile_dir_name("zz23456789abcdef-label"));
        assert!(!is_profile_dir_name("index.bin"));
    }
}
