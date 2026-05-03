//! Helper value enums and their `From` conversions for clap argument parsing.

use synrepo::config::Mode;
use synrepo::pipeline::export::ExportFormat;
use synrepo::pipeline::maintenance::CompactPolicy;
use synrepo::surface::graph_view::GraphViewDirection;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum ModeArg {
    Auto,
    Curated,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum ExportFormatArg {
    Markdown,
    Json,
    #[value(name = "graph-json")]
    GraphJson,
    #[value(name = "graph-html")]
    GraphHtml,
}

impl From<ExportFormatArg> for ExportFormat {
    fn from(arg: ExportFormatArg) -> Self {
        match arg {
            ExportFormatArg::Markdown => ExportFormat::Markdown,
            ExportFormatArg::Json => ExportFormat::Json,
            ExportFormatArg::GraphJson => ExportFormat::GraphJson,
            ExportFormatArg::GraphHtml => ExportFormat::GraphHtml,
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

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum GraphDirectionArg {
    Both,
    Inbound,
    Outbound,
}

impl From<GraphDirectionArg> for GraphViewDirection {
    fn from(arg: GraphDirectionArg) -> Self {
        match arg {
            GraphDirectionArg::Both => GraphViewDirection::Both,
            GraphDirectionArg::Inbound => GraphViewDirection::Inbound,
            GraphDirectionArg::Outbound => GraphViewDirection::Outbound,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::ValueEnum;

    use super::ExportFormatArg;
    use synrepo::pipeline::export::ExportFormat;

    #[test]
    fn export_format_arg_accepts_graph_formats() {
        let graph_json = ExportFormatArg::from_str("graph-json", true).unwrap();
        let graph_html = ExportFormatArg::from_str("graph-html", true).unwrap();

        assert_eq!(ExportFormat::from(graph_json), ExportFormat::GraphJson);
        assert_eq!(ExportFormat::from(graph_html), ExportFormat::GraphHtml);
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
