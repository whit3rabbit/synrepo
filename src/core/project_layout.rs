//! Manifest-backed project layout detection.

use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// Project family inferred from a repository manifest or conventional layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectProfileKind {
    /// Flutter application or package.
    Flutter,
    /// Dart package without a Flutter dependency.
    Dart,
    /// Rust crate or workspace.
    Rust,
    /// Go module.
    Go,
    /// JavaScript or TypeScript package.
    Node,
    /// Python package or application.
    Python,
}

impl ProjectProfileKind {
    /// Stable lowercase label for status/readiness surfaces.
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectProfileKind::Flutter => "flutter",
            ProjectProfileKind::Dart => "dart",
            ProjectProfileKind::Rust => "rust",
            ProjectProfileKind::Go => "go",
            ProjectProfileKind::Node => "node",
            ProjectProfileKind::Python => "python",
        }
    }
}

/// One detected project profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectProfile {
    /// Detected project family.
    pub kind: ProjectProfileKind,
    /// Manifest path relative to the repository root.
    pub manifest_path: String,
    /// Package name from the manifest when available.
    pub package_name: Option<String>,
}

/// Advisory project layout inferred from manifests and conventional folders.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectLayout {
    /// Detected project profiles.
    pub profiles: Vec<ProjectProfile>,
    /// Conventional source roots that exist in the repository.
    pub source_roots: Vec<String>,
    /// Conventional test roots that exist in the repository.
    pub test_roots: Vec<String>,
    /// Detected roots not covered by the active configured roots.
    pub excluded_roots: Vec<String>,
}

impl ProjectLayout {
    /// Return true when no profile or conventional root was detected.
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty() && self.source_roots.is_empty() && self.test_roots.is_empty()
    }

    /// Comma-separated profile labels for compact status details.
    pub fn profile_labels(&self) -> String {
        self.profiles
            .iter()
            .map(|profile| profile.kind.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Parsed subset of `pubspec.yaml` used by synrepo.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PubspecInfo {
    /// Dart package name, when declared.
    pub name: Option<String>,
    /// Whether regular dependencies include `flutter`.
    pub is_flutter: bool,
}

#[derive(Debug, Deserialize)]
struct PubspecManifest {
    name: Option<String>,
    dependencies: Option<BTreeMap<String, yaml_serde::Value>>,
}

/// Load the repository's `pubspec.yaml`, if present and parseable.
pub fn load_pubspec_info(repo_root: &Path) -> Option<PubspecInfo> {
    let text = fs::read_to_string(repo_root.join("pubspec.yaml")).ok()?;
    let manifest: PubspecManifest = yaml_serde::from_str(&text).ok()?;
    let is_flutter = manifest
        .dependencies
        .as_ref()
        .is_some_and(|deps| deps.contains_key("flutter"));
    Some(PubspecInfo {
        name: manifest.name,
        is_flutter,
    })
}

/// Detect project profiles and conventional roots without mutating config.
pub fn detect_project_layout(repo_root: &Path, configured_roots: &[String]) -> ProjectLayout {
    let mut profiles = Vec::new();
    let mut source_roots = BTreeSet::new();
    let mut test_roots = BTreeSet::new();

    if let Some(pubspec) = load_pubspec_info(repo_root) {
        let kind = if pubspec.is_flutter {
            ProjectProfileKind::Flutter
        } else {
            ProjectProfileKind::Dart
        };
        profiles.push(ProjectProfile {
            kind,
            manifest_path: "pubspec.yaml".to_string(),
            package_name: pubspec.name,
        });
        add_existing(repo_root, &mut source_roots, &["lib", "bin"]);
        add_existing(repo_root, &mut test_roots, &["test"]);
    }

    if repo_root.join("Cargo.toml").is_file() {
        profiles.push(ProjectProfile {
            kind: ProjectProfileKind::Rust,
            manifest_path: "Cargo.toml".to_string(),
            package_name: None,
        });
        add_existing(repo_root, &mut source_roots, &["src"]);
        add_existing(repo_root, &mut test_roots, &["tests"]);
    }

    if repo_root.join("go.mod").is_file() {
        profiles.push(ProjectProfile {
            kind: ProjectProfileKind::Go,
            manifest_path: "go.mod".to_string(),
            package_name: None,
        });
        add_go_module_roots(repo_root, &mut source_roots);
    }

    if repo_root.join("package.json").is_file() {
        profiles.push(ProjectProfile {
            kind: ProjectProfileKind::Node,
            manifest_path: "package.json".to_string(),
            package_name: None,
        });
        add_existing(
            repo_root,
            &mut source_roots,
            &["src", "app", "pages", "lib"],
        );
        add_existing(repo_root, &mut test_roots, &["test", "tests"]);
    }

    if has_python_manifest(repo_root) {
        profiles.push(ProjectProfile {
            kind: ProjectProfileKind::Python,
            manifest_path: python_manifest_path(repo_root)
                .unwrap_or("pyproject.toml")
                .to_string(),
            package_name: None,
        });
        add_existing(repo_root, &mut source_roots, &["src"]);
        add_python_package_roots(repo_root, &mut source_roots);
        add_existing(repo_root, &mut test_roots, &["test", "tests"]);
    }

    let source_roots = into_sorted(source_roots);
    let test_roots = into_sorted(test_roots);
    let excluded_roots = source_roots
        .iter()
        .chain(test_roots.iter())
        .filter(|root| !is_within_configured_roots(Path::new(root.as_str()), configured_roots))
        .cloned()
        .collect();

    ProjectLayout {
        profiles,
        source_roots,
        test_roots,
        excluded_roots,
    }
}

fn add_existing(repo_root: &Path, roots: &mut BTreeSet<String>, candidates: &[&str]) {
    for candidate in candidates {
        if repo_root.join(candidate).is_dir() {
            roots.insert((*candidate).to_string());
        }
    }
}

fn add_go_module_roots(repo_root: &Path, roots: &mut BTreeSet<String>) {
    for candidate in ["cmd", "internal", "pkg"] {
        if repo_root.join(candidate).is_dir() {
            roots.insert(candidate.to_string());
        }
    }
    if has_file_with_extension(repo_root, "go") {
        roots.insert(".".to_string());
    }
}

fn add_python_package_roots(repo_root: &Path, roots: &mut BTreeSet<String>) {
    let Ok(entries) = fs::read_dir(repo_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || is_hidden_or_runtime_dir(&path) {
            continue;
        }
        if path.join("__init__.py").is_file() {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                roots.insert(name.to_string());
            }
        }
    }
}

fn has_python_manifest(repo_root: &Path) -> bool {
    python_manifest_path(repo_root).is_some()
}

fn python_manifest_path(repo_root: &Path) -> Option<&'static str> {
    [
        "pyproject.toml",
        "setup.py",
        "setup.cfg",
        "requirements.txt",
    ]
    .into_iter()
    .find(|manifest| repo_root.join(manifest).is_file())
}

fn has_file_with_extension(dir: &Path, extension: &str) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
    })
}

fn is_hidden_or_runtime_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    name.starts_with('.')
        || matches!(
            name,
            "build" | "dist" | "node_modules" | "target" | "__pycache__"
        )
}

fn is_within_configured_roots(path: &Path, roots: &[String]) -> bool {
    roots.iter().any(|root| {
        if root == "." || root.is_empty() {
            return true;
        }
        let root_path = PathBuf::from(root);
        path == root_path || path.starts_with(root_path)
    })
}

fn into_sorted(roots: BTreeSet<String>) -> Vec<String> {
    roots.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn pubspec_detects_flutter_dependency() {
        let repo = tempdir().unwrap();
        fs::write(
            repo.path().join("pubspec.yaml"),
            "name: ottershell\ndependencies:\n  flutter:\n    sdk: flutter\n",
        )
        .unwrap();

        let info = load_pubspec_info(repo.path()).unwrap();
        assert_eq!(info.name.as_deref(), Some("ottershell"));
        assert!(info.is_flutter);
    }

    #[test]
    fn project_layout_detects_flutter_roots_and_exclusions() {
        let repo = tempdir().unwrap();
        fs::write(repo.path().join("pubspec.yaml"), "name: app\n").unwrap();
        fs::create_dir_all(repo.path().join("lib")).unwrap();
        fs::create_dir_all(repo.path().join("test")).unwrap();

        let layout = detect_project_layout(repo.path(), &["lib".to_string()]);

        assert_eq!(layout.profiles[0].kind, ProjectProfileKind::Dart);
        assert_eq!(layout.source_roots, vec!["lib".to_string()]);
        assert_eq!(layout.test_roots, vec!["test".to_string()]);
        assert_eq!(layout.excluded_roots, vec!["test".to_string()]);
    }

    #[test]
    fn project_layout_detects_node_and_python_conventions() {
        let repo = tempdir().unwrap();
        fs::write(repo.path().join("package.json"), "{}").unwrap();
        fs::write(repo.path().join("pyproject.toml"), "[project]\n").unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::create_dir_all(repo.path().join("tests")).unwrap();
        fs::create_dir_all(repo.path().join("mypkg")).unwrap();
        fs::write(repo.path().join("mypkg/__init__.py"), "").unwrap();

        let layout = detect_project_layout(repo.path(), &[".".to_string()]);

        assert!(layout
            .profiles
            .iter()
            .any(|profile| profile.kind == ProjectProfileKind::Node));
        assert!(layout
            .profiles
            .iter()
            .any(|profile| profile.kind == ProjectProfileKind::Python));
        assert!(layout.source_roots.contains(&"src".to_string()));
        assert!(layout.source_roots.contains(&"mypkg".to_string()));
        assert!(layout.test_roots.contains(&"tests".to_string()));
        assert!(layout.excluded_roots.is_empty());
    }
}
