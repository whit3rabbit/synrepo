use tempfile::tempdir;

use super::{status_output, write_malformed_config, EnvGuard};

#[test]
fn status_not_initialized_json() {
    let repo = tempdir().unwrap();
    write_malformed_config(repo.path());

    let out = status_output(repo.path(), true, false, false).unwrap();
    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(json, serde_json::json!({ "initialized": false }));
}

#[test]
fn status_not_initialized_human() {
    let repo = tempdir().unwrap();
    write_malformed_config(repo.path());

    let out = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        out.contains("synrepo status: not initialized"),
        "expected not-initialized banner, got: {out}"
    );
    assert!(
        out.contains("Run `synrepo init`"),
        "expected init hint, got: {out}"
    );
}

#[test]
fn status_truly_uninitialized_json() {
    let _env = EnvGuard::new();
    // Config::load falls back to ~/.synrepo/config.toml when the repo-local
    // file is missing; redirect HOME so the user's global config can't satisfy
    // the load and we actually test the "truly uninitialized" path.
    let home = tempdir().unwrap();
    #[cfg(unix)]
    std::env::set_var("HOME", home.path());
    #[cfg(windows)]
    std::env::set_var("USERPROFILE", home.path());

    let repo = tempdir().unwrap();
    let out = status_output(repo.path(), true, false, false).unwrap();
    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(json, serde_json::json!({ "initialized": false }));
}

#[test]
fn status_truly_uninitialized_human() {
    let _env = EnvGuard::new();
    let home = tempdir().unwrap();
    #[cfg(unix)]
    std::env::set_var("HOME", home.path());
    #[cfg(windows)]
    std::env::set_var("USERPROFILE", home.path());

    let repo = tempdir().unwrap();
    let out = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        out.contains("synrepo status: not initialized"),
        "expected not-initialized banner, got: {out}"
    );
    assert!(
        out.contains("Run `synrepo init`"),
        "expected init hint, got: {out}"
    );
}
