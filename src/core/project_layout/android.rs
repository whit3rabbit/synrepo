use super::{ProjectProfile, ProjectProfileKind};
use std::{collections::BTreeSet, fs, path::Path};

const MAX_ANDROID_SCAN_DEPTH: usize = 6;
const ANDROID_SOURCE_SETS: &[(&str, bool)] = &[
    ("src/main/java", false),
    ("src/main/kotlin", false),
    ("src/test/java", true),
    ("src/test/kotlin", true),
    ("src/androidTest/java", true),
    ("src/androidTest/kotlin", true),
];

pub(super) fn add_android_layout(
    repo_root: &Path,
    profiles: &mut Vec<ProjectProfile>,
    source_roots: &mut BTreeSet<String>,
    test_roots: &mut BTreeSet<String>,
) {
    let manifests = find_android_manifests(repo_root);
    let gradle_marker = android_gradle_marker(repo_root);
    if manifests.is_empty() && gradle_marker.is_none() {
        return;
    }

    let manifest_path = manifests
        .first()
        .map(|path| path.as_str())
        .or(gradle_marker.as_deref())
        .unwrap_or("build.gradle")
        .to_string();
    profiles.push(ProjectProfile {
        kind: ProjectProfileKind::Android,
        manifest_path,
        package_name: None,
    });

    if manifests.is_empty() {
        add_android_source_sets(repo_root, "", source_roots, test_roots);
        add_one_level_gradle_modules(repo_root, source_roots, test_roots);
        return;
    }

    for manifest in manifests {
        if let Some(module_base) = module_base_for_manifest(&manifest) {
            add_android_source_sets(repo_root, &module_base, source_roots, test_roots);
        }
    }
}

pub(crate) fn android_launcher_activities(repo_root: &Path) -> BTreeSet<String> {
    find_android_manifests(repo_root)
        .into_iter()
        .filter_map(|manifest| fs::read_to_string(repo_root.join(manifest)).ok())
        .flat_map(|text| launcher_activities_from_manifest(&text))
        .collect()
}

fn add_one_level_gradle_modules(
    repo_root: &Path,
    source_roots: &mut BTreeSet<String>,
    test_roots: &mut BTreeSet<String>,
) {
    let Ok(entries) = fs::read_dir(repo_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || skip_dir(&path) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.join("build.gradle").is_file()
            || path.join("build.gradle.kts").is_file()
            || path.join("src").is_dir()
        {
            add_android_source_sets(repo_root, name, source_roots, test_roots);
        }
    }
}

fn add_android_source_sets(
    repo_root: &Path,
    module_base: &str,
    source_roots: &mut BTreeSet<String>,
    test_roots: &mut BTreeSet<String>,
) {
    for (suffix, is_test) in ANDROID_SOURCE_SETS {
        let root = join_rel(module_base, suffix);
        if !repo_root.join(&root).is_dir() {
            continue;
        }
        if *is_test {
            test_roots.insert(root);
        } else {
            source_roots.insert(root);
        }
    }
}

fn find_android_manifests(repo_root: &Path) -> Vec<String> {
    let mut manifests = Vec::new();
    collect_android_manifests(repo_root, repo_root, 0, &mut manifests);
    manifests.sort();
    manifests
}

fn collect_android_manifests(
    repo_root: &Path,
    dir: &Path,
    depth: usize,
    manifests: &mut Vec<String>,
) {
    if depth > MAX_ANDROID_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !skip_dir(&path) {
                collect_android_manifests(repo_root, &path, depth + 1, manifests);
            }
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) != Some("AndroidManifest.xml") {
            continue;
        }
        if let Some(rel) = rel_string(repo_root, &path) {
            manifests.push(rel);
        }
    }
}

fn android_gradle_marker(repo_root: &Path) -> Option<String> {
    for candidate in [
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
    ] {
        let path = repo_root.join(candidate);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if text.contains("com.android.") {
            return Some(candidate.to_string());
        }
    }
    None
}

fn module_base_for_manifest(manifest: &str) -> Option<String> {
    if manifest == "AndroidManifest.xml" || manifest == "src/main/AndroidManifest.xml" {
        return Some(String::new());
    }
    manifest
        .strip_suffix("/src/main/AndroidManifest.xml")
        .map(|base| base.to_string())
}

fn launcher_activities_from_manifest(text: &str) -> Vec<String> {
    activity_blocks(text)
        .into_iter()
        .filter(|block| {
            block.contains("android.intent.action.MAIN")
                && block.contains("android.intent.category.LAUNCHER")
        })
        .filter_map(|block| {
            let attr = if block.starts_with("<activity-alias") {
                "android:targetActivity"
            } else {
                "android:name"
            };
            find_xml_attr(block, attr).or_else(|| find_xml_attr(block, "android:name"))
        })
        .filter_map(|name| simple_android_class_name(&name))
        .collect()
}

fn activity_blocks(text: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<activity") {
        rest = &rest[start..];
        let close_tag = if rest.starts_with("<activity-alias") {
            "</activity-alias>"
        } else {
            "</activity>"
        };
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let open_tag = &rest[..=tag_end];
        if open_tag.trim_end().ends_with("/>") {
            blocks.push(open_tag);
            rest = &rest[tag_end + 1..];
            continue;
        }
        let Some(close) = rest.find(close_tag) else {
            break;
        };
        let end = close + close_tag.len();
        blocks.push(&rest[..end]);
        rest = &rest[end..];
    }
    blocks
}

fn find_xml_attr(text: &str, attr: &str) -> Option<String> {
    let start = text.find(attr)?;
    let after_attr = &text[start + attr.len()..];
    let equals = after_attr.find('=')?;
    let after_equals = after_attr[equals + 1..].trim_start();
    let quote = after_equals.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value_start = quote.len_utf8();
    let value_end = after_equals[value_start..].find(quote)? + value_start;
    Some(after_equals[value_start..value_end].to_string())
}

fn simple_android_class_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed
        .rsplit(['.', '$'])
        .find(|part| !part.is_empty())
        .map(|part| part.to_string())
}

fn join_rel(base: &str, suffix: &str) -> String {
    if base.is_empty() {
        suffix.to_string()
    } else {
        format!("{base}/{suffix}")
    }
}

fn rel_string(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .and_then(|rel| rel.to_str())
        .map(|rel| rel.replace('\\', "/"))
}

fn skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    name.starts_with('.')
        || matches!(
            name,
            "build" | "dist" | "node_modules" | "target" | "__pycache__"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn launcher_activities_detects_manifest_main_launcher() {
        let repo = tempdir().unwrap();
        let manifest = repo.path().join("app/src/main/AndroidManifest.xml");
        fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        fs::write(
            &manifest,
            r#"<manifest package="com.example">
  <application>
    <activity android:name=".MainActivity">
      <intent-filter>
        <action android:name="android.intent.action.MAIN" />
        <category android:name="android.intent.category.LAUNCHER" />
      </intent-filter>
    </activity>
  </application>
</manifest>"#,
        )
        .unwrap();

        let launchers = android_launcher_activities(repo.path());

        assert!(launchers.contains("MainActivity"));
    }

    #[test]
    fn launcher_activities_detects_activity_alias_target() {
        let repo = tempdir().unwrap();
        let manifest = repo.path().join("app/src/main/AndroidManifest.xml");
        fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        fs::write(
            &manifest,
            r#"<manifest package="com.example">
  <application>
    <activity-alias android:name=".Alias" android:targetActivity="com.example.RealActivity">
      <intent-filter>
        <action android:name="android.intent.action.MAIN" />
        <category android:name="android.intent.category.LAUNCHER" />
      </intent-filter>
    </activity-alias>
  </application>
</manifest>"#,
        )
        .unwrap();

        let launchers = android_launcher_activities(repo.path());

        assert!(launchers.contains("RealActivity"));
    }
}
