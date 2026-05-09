use std::fmt;

use crate::cli_support::commands::StepOutcome;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApplyReport {
    title: String,
    lines: Vec<String>,
}

impl ApplyReport {
    pub(crate) fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            lines: Vec::new(),
        }
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }

    pub(crate) fn add_line(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    pub(crate) fn add_step(&mut self, label: impl AsRef<str>, outcome: StepOutcome) {
        self.add_line(format!("{}: {}", label.as_ref(), outcome_label(outcome)));
    }

    pub(crate) fn add_backup(&mut self, backup: Option<&str>) {
        match backup {
            Some(path) => self.add_line(format!("Backup: {path}")),
            None => self.add_line("Backup: not needed"),
        }
    }

    pub(crate) fn add_success(&mut self, message: impl AsRef<str>) {
        self.add_line(format!("Status: {}", message.as_ref()));
    }

    fn with_failure(&self, label: &str, error: &anyhow::Error) -> Self {
        let mut report = self.clone();
        report.add_line(format!("{label}: failed"));
        report.add_line(format!("Error: {error:#}"));
        report.add_line("Status: failed before completion");
        report
    }

    pub(crate) fn failure(&self, label: impl AsRef<str>, error: anyhow::Error) -> ApplyReportError {
        ApplyReportError::new(self.with_failure(label.as_ref(), &error), error)
    }

    pub(crate) fn record_step<F>(
        &mut self,
        label: impl AsRef<str>,
        run: F,
    ) -> Result<StepOutcome, ApplyReportError>
    where
        F: FnOnce() -> anyhow::Result<StepOutcome>,
    {
        let label = label.as_ref();
        match run() {
            Ok(outcome) => {
                self.add_step(label, outcome.clone());
                Ok(outcome)
            }
            Err(error) => Err(self.failure(label, error)),
        }
    }

    pub(crate) fn record_value<T, F>(
        &mut self,
        label: impl AsRef<str>,
        run: F,
    ) -> Result<T, ApplyReportError>
    where
        F: FnOnce() -> anyhow::Result<T>,
    {
        let label = label.as_ref();
        match run() {
            Ok(value) => Ok(value),
            Err(error) => Err(self.failure(label, error)),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ApplyReportError {
    report: ApplyReport,
    source: anyhow::Error,
}

impl ApplyReportError {
    fn new(report: ApplyReport, source: anyhow::Error) -> Self {
        Self { report, source }
    }

    pub(crate) fn report(&self) -> &ApplyReport {
        &self.report
    }

    pub(crate) fn into_anyhow(self) -> anyhow::Error {
        self.source
    }
}

impl fmt::Display for ApplyReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.source)
    }
}

impl std::error::Error for ApplyReportError {}

fn outcome_label(outcome: StepOutcome) -> &'static str {
    match outcome {
        StepOutcome::Applied => "applied",
        StepOutcome::AlreadyCurrent => "already current",
        StepOutcome::Updated => "updated",
        StepOutcome::NotAutomated => "manual setup required",
    }
}

pub(crate) fn show_apply_report_popup(
    opts: synrepo::tui::TuiOptions,
    report: &ApplyReport,
) -> anyhow::Result<()> {
    synrepo::tui::run_result_popup(opts, report.title(), report.lines())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_records_step_outcomes_and_backups() {
        let mut report = ApplyReport::new("integration complete");
        report.add_step("Shim", StepOutcome::Applied);
        report.add_step("MCP", StepOutcome::AlreadyCurrent);
        report.add_step("Hooks", StepOutcome::Updated);
        report.add_backup(Some(".mcp.json.bak"));
        report.add_success("Integration complete.");

        assert_eq!(report.title(), "integration complete");
        assert!(report.lines().contains(&"Shim: applied".to_string()));
        assert!(report.lines().contains(&"MCP: already current".to_string()));
        assert!(report.lines().contains(&"Hooks: updated".to_string()));
        assert!(report
            .lines()
            .contains(&"Backup: .mcp.json.bak".to_string()));
        assert!(report
            .lines()
            .contains(&"Status: Integration complete.".to_string()));
    }

    #[test]
    fn failed_step_returns_partial_report_with_error() {
        let mut report = ApplyReport::new("setup failed");
        report.add_step("Runtime", StepOutcome::Applied);

        let error = report
            .record_step("MCP", || anyhow::bail!("mcp blew up"))
            .expect_err("failure");

        assert!(error
            .report()
            .lines()
            .contains(&"Runtime: applied".to_string()));
        assert!(error.report().lines().contains(&"MCP: failed".to_string()));
        assert!(error
            .report()
            .lines()
            .iter()
            .any(|line| line.contains("mcp blew up")));
    }
}
