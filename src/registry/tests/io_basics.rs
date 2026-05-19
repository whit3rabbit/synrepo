use tempfile::tempdir;

#[test]
fn load_from_missing_file_returns_empty_registry() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("projects.toml");
    let registry = crate::registry::io::load_from(&path).unwrap();
    assert!(registry.projects.is_empty());
}
