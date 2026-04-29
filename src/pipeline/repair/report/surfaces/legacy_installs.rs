use agent_config::{InstallStatus, Scope, ScopeKind, StatusReport};

use crate::agent_install::{SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};
use crate::pipeline::repair::{DriftClass, RepairAction, RepairFinding, RepairSurface, Severity};

use super::{RepairContext, SurfaceCheck};

pub struct LegacyAgentInstallsCheck;

impl SurfaceCheck for LegacyAgentInstallsCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::LegacyAgentInstalls
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        let legacy = detect_legacy_installs(ctx.repo_root);
        if legacy.is_empty() {
            return vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Current,
                severity: Severity::Actionable,
                target_id: None,
                recommended_action: RepairAction::None,
                notes: None,
            }];
        }

        vec![RepairFinding {
            surface: self.surface(),
            drift_class: DriftClass::Stale,
            severity: Severity::ReportOnly,
            target_id: Some(legacy.len().to_string()),
            recommended_action: RepairAction::ManualReview,
            notes: Some(format!(
                "Legacy unowned synrepo agent install(s) detected: {}. Run `synrepo upgrade --apply` to adopt them into the agent-config ownership ledger.",
                legacy.join(", ")
            )),
        }]
    }
}

fn detect_legacy_installs(repo_root: &std::path::Path) -> Vec<String> {
    let scope = Scope::Local(repo_root.to_path_buf());
    let mut found = Vec::new();

    for mcp in agent_config::mcp_capable() {
        if !mcp.supported_mcp_scopes().contains(&ScopeKind::Local) {
            continue;
        }
        if is_present_unowned(mcp.mcp_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)) {
            found.push(format!("{}:mcp", mcp.id()));
        }
    }

    for skill in agent_config::skill_capable() {
        if !skill.supported_skill_scopes().contains(&ScopeKind::Local) {
            continue;
        }
        if is_present_unowned(skill.skill_status(
            &scope,
            SYNREPO_INSTALL_NAME,
            SYNREPO_INSTALL_OWNER,
        )) {
            found.push(format!("{}:skill", skill.id()));
        }
    }

    for instruction in agent_config::instruction_capable() {
        if !instruction
            .supported_instruction_scopes()
            .contains(&ScopeKind::Local)
        {
            continue;
        }
        if is_present_unowned(instruction.instruction_status(
            &scope,
            SYNREPO_INSTALL_NAME,
            SYNREPO_INSTALL_OWNER,
        )) {
            found.push(format!("{}:instruction", instruction.id()));
        }
    }

    found
}

fn is_present_unowned(report: Result<StatusReport, agent_config::AgentConfigError>) -> bool {
    matches!(
        report.map(|report| report.status),
        Ok(InstallStatus::PresentUnowned)
    )
}
