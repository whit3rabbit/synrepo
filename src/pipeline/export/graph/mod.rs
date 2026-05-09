//! Canonical graph export rendering.

mod html;
mod payload;
mod types;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::{structure::graph::GraphReader, surface::card::Budget};

use payload::GraphExportContext;

const GRAPH_JSON_FILENAME: &str = "graph.json";

#[derive(Clone, Copy)]
pub(super) struct GraphExportStats {
    pub(super) node_count: usize,
    pub(super) edge_count: usize,
}

pub(super) fn write_graph_json(
    export_dir: &Path,
    graph: &dyn GraphReader,
    budget: Budget,
) -> crate::Result<GraphExportStats> {
    let context = GraphExportContext::load(graph, budget)?;
    let stats = context.stats();
    write_graph_json_file(export_dir, &context)?;
    Ok(stats)
}

pub(super) fn write_graph_html(
    export_dir: &Path,
    graph: &dyn GraphReader,
    budget: Budget,
) -> crate::Result<GraphExportStats> {
    let context = GraphExportContext::load(graph, budget)?;
    let stats = context.stats();
    write_graph_json_file(export_dir, &context)?;
    html::write_graph_html_file(export_dir, |writer| context.write_compact_json(writer))?;
    Ok(stats)
}

fn write_graph_json_file(export_dir: &Path, context: &GraphExportContext<'_>) -> crate::Result<()> {
    let path = export_dir.join(GRAPH_JSON_FILENAME);
    let mut writer = BufWriter::new(File::create(path)?);
    context.write_pretty_json(&mut writer)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}
