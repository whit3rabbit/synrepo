use std::path::Path;

use synrepo::{
    surface::graph_view::{parse_edge_kind_filters, GraphNeighborhoodRequest, GraphViewDirection},
    tui::{stdout_is_tty, theme::Theme, TuiOptions},
};

use crate::cli_support::{
    cli_args::GraphCommand,
    graph::{graph_query_output, graph_stats_output, graph_view_json_output, graph_view_model},
};

pub(crate) fn graph(
    repo_root: &Path,
    command: GraphCommand,
    tui_opts: TuiOptions,
) -> anyhow::Result<()> {
    match command {
        GraphCommand::Query { q } => {
            println!("{}", graph_query_output(repo_root, &q)?);
            Ok(())
        }
        GraphCommand::Stats => {
            println!("{}", graph_stats_output(repo_root)?);
            Ok(())
        }
        GraphCommand::View {
            target,
            direction,
            edge_kind,
            depth,
            limit,
            json,
        } => {
            let request = GraphNeighborhoodRequest {
                target,
                direction: GraphViewDirection::from(direction),
                edge_types: parse_edge_kind_filters(&edge_kind)?,
                depth,
                limit,
            };
            if json {
                println!("{}", graph_view_json_output(repo_root, request)?);
                return Ok(());
            }
            if !stdout_is_tty() {
                anyhow::bail!(
                    "`synrepo graph view` requires a TTY. Use `synrepo graph view --json` for pipes and CI."
                );
            }
            let theme = Theme::from_no_color(tui_opts.no_color);
            let mut load = |request| graph_view_model(repo_root, request);
            synrepo::tui::run_graph_view(request, theme, &mut load)?;
            Ok(())
        }
    }
}
