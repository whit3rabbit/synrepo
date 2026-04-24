//! Helper value enums and their `From` conversions for clap argument parsing.

use synrepo::config::Mode;
use synrepo::pipeline::export::ExportFormat;
use synrepo::pipeline::maintenance::CompactPolicy;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum ModeArg {
    Auto,
    Curated,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum ExportFormatArg {
    Markdown,
    Json,
}

impl From<ExportFormatArg> for ExportFormat {
    fn from(arg: ExportFormatArg) -> Self {
        match arg {
            ExportFormatArg::Markdown => ExportFormat::Markdown,
            ExportFormatArg::Json => ExportFormat::Json,
        }
    }
}

impl From<ModeArg> for Mode {
    fn from(mode: ModeArg) -> Self {
        match mode {
            ModeArg::Auto => Mode::Auto,
            ModeArg::Curated => Mode::Curated,
        }
    }
}

/// Output format for `synrepo stats context`.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum StatFormatArg {
    /// Human-readable text summary (default).
    Text,
    /// Pretty-printed JSON of the raw metrics struct.
    Json,
    /// Prometheus text exposition (v0.0.4).
    Prometheus,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum CompactPolicyArg {
    Default,
    Aggressive,
    AuditHeavy,
}

impl From<CompactPolicyArg> for CompactPolicy {
    fn from(arg: CompactPolicyArg) -> Self {
        match arg {
            CompactPolicyArg::Default => CompactPolicy::Default,
            CompactPolicyArg::Aggressive => CompactPolicy::Aggressive,
            CompactPolicyArg::AuditHeavy => CompactPolicy::AuditHeavy,
        }
    }
}
