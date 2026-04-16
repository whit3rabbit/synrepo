#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use toml::Value;
    use walkdir::WalkDir;

    fn cargo_toml() -> Value {
        include_str!("../Cargo.toml")
            .parse()
            .expect("Cargo.toml must parse as TOML")
    }

    #[test]
    fn criterion_dev_dependency_is_documented_in_agents() {
        let cargo = cargo_toml();
        let has_criterion = cargo["dev-dependencies"]
            .as_table()
            .and_then(|deps| deps.get("criterion"))
            .is_some();

        assert!(
            has_criterion,
            "test assumes criterion stays in dev-dependencies"
        );

        let agents = include_str!("../AGENTS.md");
        assert!(
            agents.contains("`criterion` is available for explicit benchmark work")
                || agents.contains("`criterion` is present in `Cargo.toml`"),
            "AGENTS.md must document the criterion dependency when it is present"
        );
    }

    #[test]
    fn documented_make_check_command_exists() {
        let agents = include_str!("../AGENTS.md");
        assert!(
            agents.contains("make check"),
            "AGENTS.md no longer documents make check, update this guard"
        );

        let makefile = include_str!("../Makefile");
        assert!(
            makefile
                .lines()
                .any(|line| line.trim() == "check: fmt-check lint test"),
            "Makefile must keep the documented check target aligned"
        );
    }

    #[test]
    fn documented_git_and_watcher_dependencies_exist() {
        let cargo = cargo_toml();
        let deps = cargo["dependencies"]
            .as_table()
            .expect("Cargo.toml must contain a dependencies table");
        let agents = include_str!("../AGENTS.md");

        assert!(deps.contains_key("gix"));
        assert!(deps.contains_key("notify"));
        assert!(deps.contains_key("notify-debouncer-full"));

        assert!(agents.contains("Git history mining uses `gix`"));
        assert!(agents.contains("`notify` and `notify-debouncer-full` are in `Cargo.toml`"));
    }

    #[test]
    fn direct_gix_usage_is_centralized_in_pipeline_git_snapshot() {
        let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        // This file contains the literal string "gix::" in function names and string
        // literals, so it must be excluded from its own scan to avoid a false positive.
        let guard_file = src_root.join("docs_drift.rs");
        let mut offenders = Vec::new();
        collect_gix_offenders(&src_root, &guard_file, &mut offenders);

        let git_dir = src_root.join("pipeline/git");
        let outside: Vec<_> = offenders
            .iter()
            .filter(|o| !o.starts_with(&git_dir))
            .collect();
        assert!(
            outside.is_empty(),
            "direct gix usage must stay centralized in src/pipeline/git/; files outside: {outside:?}"
        );
    }

    #[test]
    fn foundation_status_does_not_drift() {
        let foundation = include_str!("../docs/FOUNDATION.md");

        // These features are now clearly shipped or wired, so the doc should not
        // claim they are "planned" or "hardcoded None".
        let stale_markers = [
            "FileCard.git_intelligence` is hardcoded `None`",
            "| **ChangeRiskCard** | planned |",
            "| **CallPathCard** | planned |",
            "### Recent-activity surface *(planned)*",
            "Pending: dedicated `synrepo compact`",
        ];

        for marker in &stale_markers {
            assert!(
                !foundation.contains(marker),
                "FOUNDATION.md contains stale marker: '{}'. Update the doc to match current implementation.",
                marker
            );
        }
    }

    #[test]
    fn foundation_has_invariants_table() {
        let foundation = include_str!("../docs/FOUNDATION.md");
        assert!(
            foundation.contains("## Hard invariants vs current fidelity"),
            "FOUNDATION.md must maintain the Hard Invariants vs Current Fidelity table"
        );
        assert!(
            foundation.contains("| **Separation** |"),
            "Invariants table is missing the Separation row"
        );
    }

    fn collect_gix_offenders(
        src_root: &Path,
        guard_file: &Path,
        offenders: &mut Vec<std::path::PathBuf>,
    ) {
        for entry in WalkDir::new(src_root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") || path == guard_file {
                continue;
            }
            if fs::read_to_string(path)
                .map(|c| c.contains("gix::"))
                .unwrap_or(false)
            {
                offenders.push(path.to_path_buf());
            }
        }
    }
}
