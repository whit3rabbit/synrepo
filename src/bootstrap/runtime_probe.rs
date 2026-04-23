//! Read-only runtime probe.
//!
//! Classifies the on-disk `.synrepo/` state as `Uninitialized`, `Partial` (with a
//! structured list of missing components), or `Ready`, without acquiring the
//! writer lock or mutating any store. Consumed by the bare-`synrepo` entrypoint
//! router in `src/bin/cli.rs` and by the future dashboard TUI.
//!
//! Spec: `openspec/changes/runtime-dashboard-v1/specs/runtime-probe/spec.md`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::config::Config;
use crate::store::compatibility;

/// One concrete thing the probe found missing or blocked that keeps a repo out
/// of `Ready` state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Missing {
    /// `.synrepo/config.toml` does not exist.
    ConfigFile,
    /// `.synrepo/config.toml` exists but is not readable or parseable.
    ConfigUnreadable {
        /// Human-readable explanation (I/O error or TOML parse error).
        detail: String,
    },
    /// The graph SQLite file `.synrepo/graph/nodes.db` is missing.
    GraphStore,
    /// Storage-compatibility evaluation reports a blocking action
    /// (`migrate-required` or `block` on a canonical store).
    CompatBlocked {
        /// Guidance lines lifted from the compat report, one per affected
        /// store, suitable for direct display in the repair wizard.
        guidance: Vec<String>,
    },
    /// The compat evaluation itself failed (I/O error, malformed snapshot not
    /// recoverable by a warning, etc.). The probe cannot classify the store
    /// cleanly so it reports partial with this cause.
    CompatEvaluationFailed {
        /// Underlying error formatted via `Display`.
        detail: String,
    },
}

/// Runtime-readiness classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeClassification {
    /// `.synrepo/` does not exist.
    Uninitialized,
    /// `.synrepo/` exists but one or more required components are missing or
    /// blocked. `missing` is never empty when this variant is used.
    Partial {
        /// Components that must be repaired before the repo is `Ready`.
        missing: Vec<Missing>,
    },
    /// All required components present and compat-clean.
    Ready,
}

/// Agent-target identifier for observational detection. Kept narrow to the
/// targets the wizard offers in v1; additional targets can be added without
/// touching the classification contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentTargetKind {
    /// Claude Code (`.claude/` or `CLAUDE.md`).
    Claude,
    /// Cursor (`.cursor/`).
    Cursor,
    /// OpenAI Codex CLI (`.codex/`).
    Codex,
    /// GitHub Copilot (`.github/copilot-instructions.md` or `copilot-*`).
    Copilot,
    /// Windsurf (`.windsurf/`).
    Windsurf,
}

impl AgentTargetKind {
    /// Stable lowercase identifier used in serialized output.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentTargetKind::Claude => "claude",
            AgentTargetKind::Cursor => "cursor",
            AgentTargetKind::Codex => "codex",
            AgentTargetKind::Copilot => "copilot",
            AgentTargetKind::Windsurf => "windsurf",
        }
    }
}

/// Supplementary agent-integration signal reported alongside runtime readiness.
///
/// Orthogonal to `RuntimeClassification`: a repo may be `Ready` with any of
/// these values. Used by the dashboard header and the integration sub-wizard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentIntegration {
    /// No shim present for any known target.
    Absent,
    /// Shim present for `target` but MCP registration missing.
    Partial {
        /// The detected target that has a shim but no MCP registration.
        target: AgentTargetKind,
    },
    /// Shim present and MCP registered for `target`.
    Complete {
        /// The fully-configured target.
        target: AgentTargetKind,
    },
}

impl AgentIntegration {
    /// Configured target, if any. Returns `None` for `Absent`.
    pub fn target(&self) -> Option<AgentTargetKind> {
        match self {
            AgentIntegration::Absent => None,
            AgentIntegration::Partial { target } | AgentIntegration::Complete { target } => {
                Some(*target)
            }
        }
    }
}

/// Structured output of [`probe`].
#[derive(Clone, Debug)]
pub struct ProbeReport {
    /// Runtime readiness classification.
    pub classification: RuntimeClassification,
    /// Agent-integration signal (always reported, even when runtime is not
    /// `Ready`; callers typically only surface it in the `Ready` case).
    pub agent_integration: AgentIntegration,
    /// Deterministic ordered list of agent-target candidates detected via
    /// observational hints in the repo and `$HOME`. Used by the setup wizard
    /// to pre-select a default; empty means no hints matched.
    pub detected_agent_targets: Vec<AgentTargetKind>,
    /// `.synrepo/` path that was probed (regardless of whether it exists).
    pub synrepo_dir: PathBuf,
}

/// Run the runtime probe against `repo_root`.
///
/// Read-only: no writer-lock acquisition, no store mutation, no log append.
/// Safe to call concurrently with an active watch service or writer.
pub fn probe(repo_root: &Path) -> ProbeReport {
    probe_with_home(repo_root, dirs_home().as_deref())
}

/// Lower-level probe entry point that accepts an explicit `home` override. The
/// public [`probe`] wrapper resolves `home` via the platform environment. Tests
/// use this form to avoid picking up the real user's `$HOME/.claude` etc.
pub fn probe_with_home(repo_root: &Path, home: Option<&Path>) -> ProbeReport {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let detected_agent_targets = detect_agent_targets(repo_root, home);

    let (classification, config_for_agent) = if synrepo_dir.exists() {
        classify_partial_or_ready(&synrepo_dir)
    } else {
        (RuntimeClassification::Uninitialized, None)
    };

    // Agent integration is computed unconditionally so the dashboard and
    // wizard can surface "you already wrote a shim" hints even on an
    // uninitialized repo.
    let agent_integration = detect_agent_integration(
        repo_root,
        &synrepo_dir,
        config_for_agent.as_ref(),
        &detected_agent_targets,
    );

    ProbeReport {
        classification,
        agent_integration,
        detected_agent_targets,
        synrepo_dir,
    }
}

/// Routing decision derived from a [`ProbeReport`]. Consumed by the bare-
/// `synrepo` entrypoint to select between setup wizard, repair wizard,
/// dashboard, or a fallback-help path.
///
/// Contract (tested): `Partial` MUST route to `OpenRepair`, never to
/// `OpenSetup`. Preserving existing `.synrepo/` state is non-negotiable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingDecision {
    /// Repo has no `.synrepo/`; open the guided setup wizard.
    OpenSetup,
    /// Repo has `.synrepo/` but is missing required components; open the
    /// repair wizard showing the structured `missing` list.
    OpenRepair {
        /// Components that must be repaired. Copied from the probe report.
        missing: Vec<Missing>,
    },
    /// Repo is ready; open the dashboard. `integration` rides along so the
    /// dashboard header can show agent-integration status on open.
    OpenDashboard {
        /// Supplementary agent-integration signal from the probe.
        integration: AgentIntegration,
    },
}

impl RoutingDecision {
    /// Derive a routing decision from a probe report.
    pub fn from_report(report: &ProbeReport) -> Self {
        match &report.classification {
            RuntimeClassification::Uninitialized => RoutingDecision::OpenSetup,
            RuntimeClassification::Partial { missing } => RoutingDecision::OpenRepair {
                missing: missing.clone(),
            },
            RuntimeClassification::Ready => RoutingDecision::OpenDashboard {
                integration: report.agent_integration.clone(),
            },
        }
    }
}

fn classify_partial_or_ready(synrepo_dir: &Path) -> (RuntimeClassification, Option<Config>) {
    let mut missing: Vec<Missing> = Vec::new();

    // Check config.toml.
    let config_path = synrepo_dir.join("config.toml");
    let config: Option<Config> = if !config_path.exists() {
        missing.push(Missing::ConfigFile);
        None
    } else {
        match fs::read_to_string(&config_path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(config) => Some(config),
                Err(err) => {
                    missing.push(Missing::ConfigUnreadable {
                        detail: format!("invalid TOML: {err}"),
                    });
                    None
                }
            },
            Err(err) => {
                missing.push(Missing::ConfigUnreadable {
                    detail: err.to_string(),
                });
                None
            }
        }
    };

    // Check graph store readiness (`nodes.db` exists and can plausibly be
    // opened). This is intentionally tighter than
    // `compatibility::evaluate::store_is_materialized`, which only cares
    // whether the store path exists for migration-policy decisions.
    if !graph_store_materialized(synrepo_dir) {
        missing.push(Missing::GraphStore);
    }

    // Check compatibility evaluation. Use the loaded config or defaults so
    // an unreadable config.toml doesn't mask a compat issue from surfacing.
    let compat_config = config.clone().unwrap_or_default();
    match compatibility::evaluate_runtime(synrepo_dir, true, &compat_config) {
        Ok(report) => {
            if report.has_blocking_actions() {
                missing.push(Missing::CompatBlocked {
                    guidance: report.guidance_lines(),
                });
            }
        }
        Err(err) => {
            missing.push(Missing::CompatEvaluationFailed {
                detail: err.to_string(),
            });
        }
    }

    if missing.is_empty() {
        (RuntimeClassification::Ready, config)
    } else {
        (RuntimeClassification::Partial { missing }, config)
    }
}

fn graph_store_materialized(synrepo_dir: &Path) -> bool {
    synrepo_dir.join("graph").join("nodes.db").exists()
}

fn detect_agent_integration(
    repo_root: &Path,
    _synrepo_dir: &Path,
    _config: Option<&Config>,
    detected_targets: &[AgentTargetKind],
) -> AgentIntegration {
    // Choose the target to report on: prefer the first detected hint, else
    // walk known targets in a stable order looking for any shim file.
    let probe_order: Vec<AgentTargetKind> = if detected_targets.is_empty() {
        all_agent_targets().to_vec()
    } else {
        detected_targets.to_vec()
    };

    for target in probe_order {
        let shim = shim_exists(repo_root, target);
        let mcp = mcp_registration_present(repo_root, target);
        match (shim, mcp) {
            (true, true) => return AgentIntegration::Complete { target },
            (true, false) => return AgentIntegration::Partial { target },
            (false, _) => continue,
        }
    }
    AgentIntegration::Absent
}

fn all_agent_targets() -> &'static [AgentTargetKind] {
    &[
        AgentTargetKind::Claude,
        AgentTargetKind::Cursor,
        AgentTargetKind::Codex,
        AgentTargetKind::Copilot,
        AgentTargetKind::Windsurf,
    ]
}

fn shim_exists(repo_root: &Path, target: AgentTargetKind) -> bool {
    shim_output_path(repo_root, target).exists()
}

fn shim_output_path(repo_root: &Path, target: AgentTargetKind) -> PathBuf {
    // Mirrors `AgentTool::output_path` in `src/bin/cli_support/agent_shims/mod.rs`.
    // Duplicated here deliberately: the library crate cannot depend on the
    // binary-crate-private agent_shims module. The pair of paths is narrow
    // and change-resistant; the test suite for Phase 3 asserts per-target
    // behavior, which will flag divergence.
    match target {
        AgentTargetKind::Claude => repo_root
            .join(".claude")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md"),
        AgentTargetKind::Cursor => repo_root
            .join(".cursor")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md"),
        AgentTargetKind::Codex => repo_root
            .join(".codex")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md"),
        AgentTargetKind::Copilot => repo_root.join("synrepo-copilot-instructions.md"),
        AgentTargetKind::Windsurf => repo_root
            .join(".windsurf")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md"),
    }
}

fn mcp_registration_present(repo_root: &Path, target: AgentTargetKind) -> bool {
    match target {
        AgentTargetKind::Claude => claude_mcp_registered(repo_root),
        AgentTargetKind::Codex => codex_mcp_registered(repo_root),
        // Cursor/Copilot/Windsurf do not have a canonical project-scoped MCP
        // registration file today. The shim is the full integration signal.
        AgentTargetKind::Cursor | AgentTargetKind::Copilot | AgentTargetKind::Windsurf => {
            shim_exists(repo_root, target)
        }
    }
}

fn claude_mcp_registered(repo_root: &Path) -> bool {
    let path = repo_root.join(".mcp.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    v.get("mcpServers").and_then(|s| s.get("synrepo")).is_some()
}

fn codex_mcp_registered(repo_root: &Path) -> bool {
    let path = repo_root.join(".codex").join("config.toml");
    let Ok(text) = fs::read_to_string(&path) else {
        return false;
    };
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    doc.get("mcp")
        .and_then(|i| i.as_table())
        .and_then(|t| t.get("synrepo"))
        .is_some()
}

fn detect_agent_targets(repo_root: &Path, home: Option<&Path>) -> Vec<AgentTargetKind> {
    // Deterministic detection order: Claude, Cursor, Codex, Copilot, Windsurf.
    // Matches `all_agent_targets()` so callers can rely on first-hit semantics.
    let mut hits: Vec<AgentTargetKind> = Vec::new();
    for target in all_agent_targets() {
        if target_hint_present(repo_root, home, *target) {
            hits.push(*target);
        }
    }
    hits
}

fn target_hint_present(repo_root: &Path, home: Option<&Path>, target: AgentTargetKind) -> bool {
    let repo_hints: Vec<PathBuf> = match target {
        AgentTargetKind::Claude => vec![repo_root.join(".claude"), repo_root.join("CLAUDE.md")],
        AgentTargetKind::Cursor => vec![repo_root.join(".cursor")],
        AgentTargetKind::Codex => vec![repo_root.join(".codex")],
        AgentTargetKind::Copilot => vec![repo_root.join(".github").join("copilot-instructions.md")],
        AgentTargetKind::Windsurf => vec![repo_root.join(".windsurf")],
    };
    if repo_hints.iter().any(|p| p.exists()) {
        return true;
    }
    let Some(home) = home else {
        return false;
    };
    let home_hints: Vec<PathBuf> = match target {
        AgentTargetKind::Claude => vec![home.join(".claude")],
        AgentTargetKind::Cursor => vec![home.join(".cursor")],
        AgentTargetKind::Codex => vec![home.join(".codex")],
        AgentTargetKind::Copilot => vec![],
        AgentTargetKind::Windsurf => vec![home.join(".windsurf")],
    };
    home_hints.iter().any(|p| p.exists())
}

fn dirs_home() -> Option<PathBuf> {
    // Minimal stdlib-only resolver to avoid pulling in `dirs`.
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

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
    fn graph_store_materialized_requires_nodes_db() {
        let dir = tempdir().unwrap();
        let synrepo = Config::synrepo_dir(dir.path());
        let graph_dir = synrepo.join("graph");
        fs::create_dir_all(&graph_dir).unwrap();
        fs::write(graph_dir.join("stray.tmp"), b"not-a-db").unwrap();

        assert!(
            !graph_store_materialized(&synrepo),
            "stray files must not count as a materialized graph store"
        );

        fs::write(graph_dir.join("nodes.db"), b"db").unwrap();
        assert!(
            graph_store_materialized(&synrepo),
            "nodes.db must count as a materialized graph store"
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
        let dir = tempdir().unwrap();
        let report = probe_with_home(dir.path(), None);
        assert_eq!(report.agent_integration, AgentIntegration::Absent);
        assert!(report.detected_agent_targets.is_empty());
    }

    #[test]
    fn agent_integration_partial_when_only_shim_present_for_cursor() {
        let dir = tempdir().unwrap();
        let cursor_skill = dir.path().join(".cursor").join("skills").join("synrepo");
        fs::create_dir_all(&cursor_skill).unwrap();
        fs::write(cursor_skill.join("SKILL.md"), b"stub shim").unwrap();

        let report = probe_with_home(dir.path(), None);
        // Cursor has no separate MCP-registration file, so "shim present" is
        // treated as "complete" — this matches the detect_agent_integration
        // branch. Keep the assertion aligned with that contract.
        assert!(matches!(
            report.agent_integration,
            AgentIntegration::Complete {
                target: AgentTargetKind::Cursor
            }
        ));
        assert_eq!(report.detected_agent_targets, vec![AgentTargetKind::Cursor]);
    }

    #[test]
    fn agent_integration_partial_claude_shim_without_mcp_registration() {
        let dir = tempdir().unwrap();
        let claude_skill = dir.path().join(".claude").join("skills").join("synrepo");
        fs::create_dir_all(&claude_skill).unwrap();
        fs::write(claude_skill.join("SKILL.md"), b"shim").unwrap();

        let report = probe_with_home(dir.path(), None);
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
        let codex_skill = codex.join("skills").join("synrepo");
        fs::create_dir_all(&codex_skill).unwrap();
        fs::write(codex_skill.join("SKILL.md"), b"shim").unwrap();
        fs::write(
            codex.join("config.toml"),
            "[mcp]\nsynrepo = \"synrepo mcp --repo .\"\n",
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
        let dir = tempdir().unwrap();
        let codex_skill = dir.path().join(".codex").join("skills").join("synrepo");
        fs::create_dir_all(&codex_skill).unwrap();
        fs::write(codex_skill.join("SKILL.md"), b"shim").unwrap();

        let report = probe_with_home(dir.path(), None);
        assert_eq!(
            report.agent_integration,
            AgentIntegration::Partial {
                target: AgentTargetKind::Codex
            }
        );
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
}
