use super::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[path = "tests/agent_integration.rs"]
mod agent_integration;

fn isolated_home() -> (
    crate::test_support::GlobalTestLock,
    tempfile::TempDir,
    PathBuf,
    crate::config::test_home::HomeEnvGuard,
) {
    let lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let canonical_home = home.path().canonicalize().unwrap();
    let guard = crate::config::test_home::HomeEnvGuard::redirect_to(&canonical_home);
    (lock, home, canonical_home, guard)
}

fn snapshot_dir_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    if !root.exists() {
        return out;
    }
    for entry in walkdir::WalkDir::new(root).sort_by_file_name() {
        let entry = entry.expect("walkdir entry");
        if entry.file_type().is_file() {
            let bytes = fs::read(entry.path()).unwrap_or_default();
            out.insert(entry.path().to_path_buf(), bytes);
        } else if entry.file_type().is_dir() {
            out.insert(entry.path().to_path_buf(), Vec::new());
        }
    }
    out
}

#[test]
fn fresh_repo_classifies_as_uninitialized() {
    let dir = tempdir().unwrap();
    let report = probe_with_home(dir.path(), None);
    assert_eq!(report.classification, RuntimeClassification::Uninitialized);
    // Probe must not create `.synrepo/` as a side effect.
    assert!(!dir.path().join(".synrepo").exists());
}

#[test]
fn missing_config_classifies_as_partial() {
    let dir = tempdir().unwrap();
    let synrepo = Config::synrepo_dir(dir.path());
    fs::create_dir_all(synrepo.join("graph")).unwrap();
    fs::write(synrepo.join("graph").join("nodes.db"), b"stub").unwrap();

    let report = probe_with_home(dir.path(), None);
    let missing = match report.classification {
        RuntimeClassification::Partial { missing } => missing,
        other => panic!("expected Partial, got {other:?}"),
    };
    assert!(missing.contains(&Missing::ConfigFile));
}

#[test]
fn unreadable_config_classifies_as_partial_with_reason() {
    let dir = tempdir().unwrap();
    let synrepo = Config::synrepo_dir(dir.path());
    fs::create_dir_all(synrepo.join("graph")).unwrap();
    fs::write(synrepo.join("graph").join("nodes.db"), b"stub").unwrap();
    fs::write(synrepo.join("config.toml"), "not = valid = toml =").unwrap();

    let report = probe_with_home(dir.path(), None);
    let missing = match report.classification {
        RuntimeClassification::Partial { missing } => missing,
        other => panic!("expected Partial, got {other:?}"),
    };
    assert!(
        missing
            .iter()
            .any(|m| matches!(m, Missing::ConfigUnreadable { .. })),
        "expected ConfigUnreadable in {missing:?}"
    );
}

#[test]
fn missing_graph_store_classifies_as_partial() {
    let dir = tempdir().unwrap();
    let synrepo = Config::synrepo_dir(dir.path());
    fs::create_dir_all(&synrepo).unwrap();
    fs::write(
        synrepo.join("config.toml"),
        toml::to_string(&Config::default()).unwrap(),
    )
    .unwrap();

    let report = probe_with_home(dir.path(), None);
    let missing = match report.classification {
        RuntimeClassification::Partial { missing } => missing,
        other => panic!("expected Partial, got {other:?}"),
    };
    assert!(missing.contains(&Missing::GraphStore));
}

#[test]
fn graph_store_materialized_requires_openable_db() {
    let dir = tempdir().unwrap();
    let synrepo = Config::synrepo_dir(dir.path());
    let graph_dir = synrepo.join("graph");
    fs::create_dir_all(&graph_dir).unwrap();
    fs::write(graph_dir.join("stray.tmp"), b"not-a-db").unwrap();

    assert!(
        !graph_store_materialized(&synrepo),
        "stray files must not count as a materialized graph store"
    );

    // A non-SQLite file at the canonical path must not count as materialized.
    // Pre-fix this would have classified as Ready and dumped the user into
    // the dashboard before failing on the first graph read.
    fs::write(graph_dir.join("nodes.db"), b"db").unwrap();
    assert!(
        !graph_store_materialized(&synrepo),
        "junk content at nodes.db must fail openability validation"
    );

    // A real SQLite-formatted file at the path must count as materialized.
    fs::remove_file(graph_dir.join("nodes.db")).unwrap();
    drop(SqliteGraphStore::open(&graph_dir).expect("create real graph store"));
    assert!(
        graph_store_materialized(&synrepo),
        "an openable nodes.db must count as a materialized graph store"
    );
}

#[test]
fn graph_store_with_unopenable_db_classifies_as_partial() {
    // Regression test for the runtime-probe ↔ graph-store openability
    // mismatch: `.synrepo/graph/nodes.db` exists but is not a valid SQLite
    // database (e.g. interrupted bootstrap, disk full mid-write, or a junk
    // file mistakenly named `nodes.db`). The probe must classify this as
    // Partial with `Missing::GraphStore`, not Ready.
    let dir = tempdir().unwrap();
    let synrepo = Config::synrepo_dir(dir.path());
    fs::create_dir_all(synrepo.join("graph")).unwrap();
    fs::write(
        synrepo.join("config.toml"),
        toml::to_string(&Config::default()).unwrap(),
    )
    .unwrap();
    fs::write(
        synrepo.join("graph").join("nodes.db"),
        b"not a sqlite database\n",
    )
    .unwrap();

    let report = probe_with_home(dir.path(), None);
    let missing = match report.classification {
        RuntimeClassification::Partial { missing } => missing,
        other => panic!("expected Partial, got {other:?}"),
    };
    assert!(
        missing.contains(&Missing::GraphStore),
        "unopenable nodes.db must surface Missing::GraphStore; got {missing:?}"
    );
}

#[test]
fn fully_populated_repo_classifies_as_ready() {
    // A minimal "ready" layout from the probe's perspective: config.toml +
    // a materialized graph/nodes.db + compat-clean snapshot. We reuse the
    // real bootstrap() helper so the runtime is exactly what `synrepo init`
    // would produce.
    let dir = tempdir().unwrap();
    let _report = crate::bootstrap::bootstrap(dir.path(), None, false).unwrap();

    let report = probe_with_home(dir.path(), None);
    assert_eq!(
        report.classification,
        RuntimeClassification::Ready,
        "bootstrap output should probe as Ready"
    );
}

#[test]
fn probe_is_read_only() {
    let dir = tempdir().unwrap();
    // Construct a partial state so the probe actually hits config-load and
    // compat-evaluate paths (these are the most likely places for
    // accidental writes).
    let synrepo = Config::synrepo_dir(dir.path());
    fs::create_dir_all(synrepo.join("graph")).unwrap();
    fs::write(synrepo.join("graph").join("nodes.db"), b"stub").unwrap();
    fs::write(synrepo.join("config.toml"), "mode = \"auto\"\n").unwrap();

    let before = snapshot_dir_bytes(dir.path());
    let _ = probe(dir.path());
    let after = snapshot_dir_bytes(dir.path());
    assert_eq!(before, after, "probe mutated the filesystem");
}

#[test]
fn routing_partial_opens_repair_not_setup() {
    // Contract test from Phase 4.2: Partial classification must route to
    // OpenRepair, never to OpenSetup. Preserving .synrepo/ state matters.
    let report = ProbeReport {
        classification: RuntimeClassification::Partial {
            missing: vec![Missing::ConfigFile],
        },
        agent_integration: AgentIntegration::Absent,
        detected_agent_targets: Vec::new(),
        synrepo_dir: PathBuf::from("/tmp/nonexistent/.synrepo"),
    };
    match RoutingDecision::from_report(&report) {
        RoutingDecision::OpenRepair { missing } => {
            assert_eq!(missing, vec![Missing::ConfigFile]);
        }
        other => panic!("expected OpenRepair, got {other:?}"),
    }
}

#[test]
fn routing_uninitialized_opens_setup() {
    let report = ProbeReport {
        classification: RuntimeClassification::Uninitialized,
        agent_integration: AgentIntegration::Absent,
        detected_agent_targets: Vec::new(),
        synrepo_dir: PathBuf::from("/tmp/x/.synrepo"),
    };
    assert_eq!(
        RoutingDecision::from_report(&report),
        RoutingDecision::OpenSetup
    );
}

#[test]
fn routing_ready_opens_dashboard_with_integration() {
    let report = ProbeReport {
        classification: RuntimeClassification::Ready,
        agent_integration: AgentIntegration::Complete {
            target: AgentTargetKind::Claude,
        },
        detected_agent_targets: vec![AgentTargetKind::Claude],
        synrepo_dir: PathBuf::from("/tmp/x/.synrepo"),
    };
    match RoutingDecision::from_report(&report) {
        RoutingDecision::OpenDashboard { integration } => {
            assert_eq!(
                integration,
                AgentIntegration::Complete {
                    target: AgentTargetKind::Claude
                }
            );
        }
        other => panic!("expected OpenDashboard, got {other:?}"),
    }
}

#[test]
fn agent_integration_absent_on_empty_repo() {
    let (_lock, _home, home_path, _guard) = isolated_home();
    let dir = tempdir().unwrap();
    let report = probe_with_home(dir.path(), Some(&home_path));
    assert_eq!(report.agent_integration, AgentIntegration::Absent);
    assert!(report.detected_agent_targets.is_empty());
}

#[test]
fn agent_integration_partial_when_only_shim_present_for_cursor() {
    let (_lock, _home, home_path, _guard) = isolated_home();
    let dir = tempdir().unwrap();
    let cursor_skill = dir.path().join(".cursor").join("skills").join("synrepo");
    fs::create_dir_all(&cursor_skill).unwrap();
    fs::write(cursor_skill.join("SKILL.md"), b"stub shim").unwrap();

    let report = probe_with_home(dir.path(), Some(&home_path));
    assert_eq!(
        report.agent_integration,
        AgentIntegration::Partial {
            target: AgentTargetKind::Cursor
        }
    );
    assert_eq!(report.detected_agent_targets, vec![AgentTargetKind::Cursor]);
}

#[test]
fn agent_integration_partial_claude_shim_without_mcp_registration() {
    let (_lock, _home, home_path, _guard) = isolated_home();
    let dir = tempdir().unwrap();
    let claude_skill = dir.path().join(".claude").join("skills").join("synrepo");
    fs::create_dir_all(&claude_skill).unwrap();
    fs::write(claude_skill.join("SKILL.md"), b"shim").unwrap();

    let report = probe_with_home(dir.path(), Some(&home_path));
    assert_eq!(
        report.agent_integration,
        AgentIntegration::Partial {
            target: AgentTargetKind::Claude
        }
    );
}

#[test]
fn agent_integration_complete_claude_with_mcp_registration() {
    let dir = tempdir().unwrap();
    let claude_skill = dir.path().join(".claude").join("skills").join("synrepo");
    fs::create_dir_all(&claude_skill).unwrap();
    fs::write(claude_skill.join("SKILL.md"), b"shim").unwrap();
    fs::write(
        dir.path().join(".mcp.json"),
        r#"{"mcpServers":{"synrepo":{"command":"synrepo","args":["mcp","--repo","."]}}}"#,
    )
    .unwrap();

    let report = probe_with_home(dir.path(), None);
    assert_eq!(
        report.agent_integration,
        AgentIntegration::Complete {
            target: AgentTargetKind::Claude
        }
    );
}

#[test]
fn agent_integration_complete_codex_requires_toml_entry() {
    let dir = tempdir().unwrap();
    let codex = dir.path().join(".codex");
    fs::create_dir_all(&codex).unwrap();
    let codex_skill = dir.path().join(".agents").join("skills").join("synrepo");
    fs::create_dir_all(&codex_skill).unwrap();
    fs::write(codex_skill.join("SKILL.md"), b"shim").unwrap();
    fs::write(
        codex.join("config.toml"),
        "[mcp_servers.synrepo]\ncommand = \"synrepo\"\nargs = [\"mcp\", \"--repo\", \".\"]\n",
    )
    .unwrap();

    let report = probe_with_home(dir.path(), None);
    assert_eq!(
        report.agent_integration,
        AgentIntegration::Complete {
            target: AgentTargetKind::Codex
        }
    );
}

#[test]
fn agent_integration_codex_shim_only_is_partial() {
    let (_lock, _home, home_path, _guard) = isolated_home();
    let dir = tempdir().unwrap();
    let codex_skill = dir.path().join(".agents").join("skills").join("synrepo");
    fs::create_dir_all(&codex_skill).unwrap();
    fs::write(codex_skill.join("SKILL.md"), b"shim").unwrap();

    let report = probe_with_home(dir.path(), Some(&home_path));
    assert_eq!(
        report.agent_integration,
        AgentIntegration::Partial {
            target: AgentTargetKind::Codex
        }
    );
}

#[test]
fn agent_target_detection_codex_agents_skills_hint() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".agents").join("skills")).unwrap();

    let report = probe_with_home(dir.path(), None);
    assert_eq!(report.detected_agent_targets, vec![AgentTargetKind::Codex]);
}

#[test]
fn agent_target_detection_multiple_hints_ordered() {
    let dir = tempdir().unwrap();
    // Deliberately create in a reversed order to prove the detection
    // order is stable and independent of creation order.
    fs::create_dir_all(dir.path().join(".windsurf")).unwrap();
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    fs::create_dir_all(dir.path().join(".cursor")).unwrap();

    let report = probe_with_home(dir.path(), None);
    assert_eq!(
        report.detected_agent_targets,
        vec![
            AgentTargetKind::Claude,
            AgentTargetKind::Cursor,
            AgentTargetKind::Windsurf,
        ]
    );
}

#[test]
fn agent_target_detection_no_hints_returns_empty() {
    let dir = tempdir().unwrap();
    let report = probe_with_home(dir.path(), None);
    assert!(report.detected_agent_targets.is_empty());
}
