#[test]
fn repo_flag_is_global_and_survives_on_every_subcommand() {
    // Spot-check a couple of subcommands; the flag is declared `global = true`
    // on `Cli`, so clap propagates it regardless of the subcommand that
    // follows. Asserting on two representative subcommands pins that
    // invariant without exploding into N x M tests.
    let status = super::parse(&["--repo", "/tmp/x", "status"]);
    assert_eq!(
        status.repo.as_deref(),
        Some(std::path::Path::new("/tmp/x")),
        "--repo must propagate to status"
    );
    let watch = super::parse(&["--repo", "/tmp/y", "watch", "--daemon"]);
    assert_eq!(
        watch.repo.as_deref(),
        Some(std::path::Path::new("/tmp/y")),
        "--repo must propagate to watch"
    );
}

#[test]
fn no_color_flag_is_global_across_subcommands() {
    let bare = super::parse(&["--no-color"]);
    assert!(bare.no_color, "--no-color should set on bare synrepo");
    let dashboard = super::parse(&["--no-color", "dashboard"]);
    assert!(dashboard.no_color, "--no-color should survive on dashboard");
}
