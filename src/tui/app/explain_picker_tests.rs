use super::*;
use std::collections::BTreeMap;
use std::fs as std_fs;
use tempfile::tempdir;

#[test]
fn first_path_segment_basics() {
    assert_eq!(first_path_segment("src/a.rs"), Some("src".to_string()));
    assert_eq!(
        first_path_segment("crates/core/x.rs"),
        Some("crates".to_string())
    );
    assert_eq!(first_path_segment("top.txt"), None);
    assert_eq!(first_path_segment("src\\win.rs"), Some("src".to_string()));
}

#[test]
fn normalize_prefix_appends_trailing_slash() {
    assert_eq!(normalize_prefix("src"), "src/".to_string());
    assert_eq!(normalize_prefix("src/"), "src/".to_string());
    assert_eq!(normalize_prefix("docs\\adr"), "docs/adr/".to_string());
}

#[test]
fn enumerate_buckets_by_top_level_segment() {
    let repo = tempdir().unwrap();
    std_fs::create_dir_all(repo.path().join("src")).unwrap();
    std_fs::create_dir_all(repo.path().join("docs")).unwrap();
    std_fs::write(repo.path().join("src/lib.rs"), "pub fn a() {}\n").unwrap();
    std_fs::write(repo.path().join("src/util.rs"), "pub fn b() {}\n").unwrap();
    std_fs::write(repo.path().join("docs/guide.md"), "# guide\n").unwrap();

    let entries = enumerate_folders(repo.path(), &Config::default()).unwrap();
    let by_path: BTreeMap<_, _> = entries.iter().map(|e| (e.path.as_str(), e)).collect();

    let src = by_path["src/"];
    assert_eq!(src.indexable_count, 2);
    assert_eq!(src.supported_count, 2);
    assert!(src.checked, "src has parser-supported files, auto-checked");

    let docs = by_path["docs/"];
    assert_eq!(docs.indexable_count, 1);
    assert_eq!(docs.supported_count, 0);
    assert!(!docs.checked, "docs has no parser-supported files");
}

#[test]
fn prior_selection_overrides_heuristic() {
    let entries = vec![
        FolderEntry {
            path: "src/".to_string(),
            indexable_count: 2,
            supported_count: 2,
            checked: true,
        },
        FolderEntry {
            path: "docs/".to_string(),
            indexable_count: 1,
            supported_count: 0,
            checked: false,
        },
    ];
    let prior = vec!["docs/".to_string()];
    let merged = apply_selection(entries, Some(&prior));
    assert!(!merged[0].checked, "src removed from prior, unchecks");
    assert!(merged[1].checked, "docs in prior, force-checks");
}

#[test]
fn save_and_load_scope_roundtrip() {
    let repo = tempdir().unwrap();
    assert!(load_scope(repo.path()).is_none());
    save_scope(repo.path(), &["src/".to_string(), "docs/".to_string()]).unwrap();
    let loaded = load_scope(repo.path()).unwrap();
    assert_eq!(loaded, vec!["src/".to_string(), "docs/".to_string()]);
}

#[test]
fn load_scope_returns_none_on_malformed_json() {
    let repo = tempdir().unwrap();
    std_fs::create_dir_all(repo.path().join(".synrepo/state")).unwrap();
    std_fs::write(repo.path().join(SCOPE_FILE), "{not json").unwrap();
    assert!(load_scope(repo.path()).is_none());
    assert!(load_scope_state(repo.path()).invalid);
}

#[test]
fn cursor_movement_clamps() {
    let mut state = FolderPickerState {
        folders: vec![
            FolderEntry {
                path: "a/".to_string(),
                indexable_count: 1,
                supported_count: 1,
                checked: false,
            },
            FolderEntry {
                path: "b/".to_string(),
                indexable_count: 1,
                supported_count: 1,
                checked: false,
            },
        ],
        cursor: 0,
    };
    state.cursor_down(10);
    assert_eq!(state.cursor, 1);
    state.cursor_up(10);
    assert_eq!(state.cursor, 0);
    state.toggle_cursor();
    assert!(state.folders[0].checked);
    assert_eq!(state.selected_paths(), vec!["a/".to_string()]);
}
