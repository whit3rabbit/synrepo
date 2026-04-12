use std::path::Path;

use anyhow::anyhow;

use crate::{
    config::Config,
    pipeline::{
        maintenance::{execute_maintenance, plan_maintenance},
        watch::{persist_reconcile_state, run_reconcile_pass},
        writer::now_rfc3339,
    },
};

use super::{
    append_resolution_log,
    report::assemble_repair_report,
    RepairAction, RepairFinding, ResolutionLogEntry, Severity, SyncOutcome, SyncSummary,
};

/// Execute a targeted sync: repair auto-fixable findings from `build_repair_report`.
///
/// Routes storage repairs through `plan_maintenance` / `execute_maintenance`
/// and structural refreshes through `run_reconcile_pass`. Report-only,
/// unsupported, and blocked findings are collected but left untouched.
/// Appends a resolution log entry to `.synrepo/state/repair-log.jsonl`.
pub fn execute_sync(
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
) -> crate::Result<SyncSummary> {
    let now = now_rfc3339();
    let maint_plan = plan_maintenance(synrepo_dir, config);
    let report = assemble_repair_report(synrepo_dir, config, &maint_plan);

    let mut repaired: Vec<RepairFinding> = Vec::new();
    let mut report_only: Vec<RepairFinding> = Vec::new();
    let mut blocked: Vec<RepairFinding> = Vec::new();
    let mut actions_taken: Vec<String> = Vec::new();
    let action_context = ActionContext {
        repo_root,
        synrepo_dir,
        config,
        maint_plan: &maint_plan,
    };

    for finding in &report.findings {
        match finding.severity {
            Severity::Blocked => blocked.push(finding.clone()),
            Severity::ReportOnly | Severity::Unsupported => report_only.push(finding.clone()),
            Severity::Actionable => {
                handle_actionable_finding(
                    finding,
                    &action_context,
                    &mut repaired,
                    &mut report_only,
                    &mut actions_taken,
                )?;
            }
        }
    }

    let outcome = if blocked.is_empty() {
        SyncOutcome::Completed
    } else {
        SyncOutcome::Partial
    };

    let entry = ResolutionLogEntry {
        synced_at: now.clone(),
        source_revision: None,
        requested_scope: report.findings.iter().map(|f| f.surface).collect(),
        findings_considered: report.findings,
        actions_taken,
        outcome,
    };
    append_resolution_log(synrepo_dir, &entry);

    Ok(SyncSummary {
        synced_at: now,
        repaired,
        report_only,
        blocked,
    })
}

struct ActionContext<'a> {
    repo_root: &'a Path,
    synrepo_dir: &'a Path,
    config: &'a Config,
    maint_plan: &'a crate::Result<crate::pipeline::maintenance::MaintenancePlan>,
}

fn handle_actionable_finding(
    finding: &RepairFinding,
    context: &ActionContext<'_>,
    repaired: &mut Vec<RepairFinding>,
    report_only: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    match finding.recommended_action {
        RepairAction::None => {}
        RepairAction::RunMaintenance => {
            let plan = context.maint_plan.as_ref().map_err(|e| anyhow!("{e}"))?;
            if plan.has_work() {
                execute_maintenance(context.synrepo_dir, plan)?;
                actions_taken.push(format!(
                    "ran storage maintenance for {}",
                    finding.surface.as_str()
                ));
            }
            repaired.push(finding.clone());
        }
        RepairAction::RunMaintenanceThenReconcile => {
            let plan = context.maint_plan.as_ref().map_err(|e| anyhow!("{e}"))?;
            if plan.has_work() {
                execute_maintenance(context.synrepo_dir, plan)?;
                actions_taken.push(format!(
                    "ran storage maintenance for {}",
                    finding.surface.as_str()
                ));
            }
            run_reconcile_and_record(
                context.repo_root,
                context.synrepo_dir,
                context.config,
                actions_taken,
            );
            repaired.push(finding.clone());
        }
        RepairAction::RunReconcile => {
            let outcome = run_reconcile_pass(context.repo_root, context.config, context.synrepo_dir);
            persist_reconcile_state(context.synrepo_dir, &outcome, 0);
            actions_taken.push(format!(
                "ran structural reconcile for {}",
                finding.surface.as_str()
            ));
            repaired.push(finding.clone());
        }
        RepairAction::ManualReview | RepairAction::NotSupported => {
            report_only.push(finding.clone());
        }
    }
    Ok(())
}

fn run_reconcile_and_record(
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    actions_taken: &mut Vec<String>,
) {
    let outcome = run_reconcile_pass(repo_root, config, synrepo_dir);
    persist_reconcile_state(synrepo_dir, &outcome, 0);
    actions_taken.push("ran structural reconcile after maintenance".to_string());
}
