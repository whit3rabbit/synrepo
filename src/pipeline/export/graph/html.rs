//! Self-contained HTML renderer for canonical graph exports.

mod script;
mod shell;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use super::GraphExport;

const GRAPH_HTML_FILENAME: &str = "graph.html";

pub(super) fn write_graph_html_file(export_dir: &Path, graph: &GraphExport) -> crate::Result<()> {
    let json = serde_json::to_string(graph)
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("graph HTML data serialize failed: {e}")))?
        .replace("</", "<\\/");
    let mut writer = BufWriter::new(File::create(export_dir.join(GRAPH_HTML_FILENAME))?);
    writer.write_all(shell::HTML_PREFIX.as_bytes())?;
    writer.write_all(json.as_bytes())?;
    for chunk in script::HTML_SUFFIX {
        writer.write_all(chunk.as_bytes())?;
    }
    writer.flush()?;
    Ok(())
}
