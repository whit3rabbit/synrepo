//! Self-contained HTML renderer for canonical graph exports.

mod script;
mod shell;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use super::GraphExport;

const GRAPH_HTML_FILENAME: &str = "graph.html";

pub(super) fn write_graph_html_file(export_dir: &Path, graph: &GraphExport) -> crate::Result<()> {
    let mut writer = BufWriter::new(File::create(export_dir.join(GRAPH_HTML_FILENAME))?);
    write_graph_html_to_writer(&mut writer, graph)?;
    writer.flush()?;
    Ok(())
}

fn write_graph_html_to_writer<W: Write>(writer: &mut W, graph: &GraphExport) -> crate::Result<()> {
    writer.write_all(shell::HTML_PREFIX.as_bytes())?;
    {
        let mut escaping = HtmlJsonEscapingWriter::new(&mut *writer);
        serde_json::to_writer(&mut escaping, graph).map_err(|e| {
            crate::Error::Other(anyhow::anyhow!("graph HTML data serialize failed: {e}"))
        })?;
        escaping.finish()?;
    }
    for chunk in script::HTML_SUFFIX {
        writer.write_all(chunk.as_bytes())?;
    }
    Ok(())
}

struct HtmlJsonEscapingWriter<W> {
    inner: W,
    pending_lt: bool,
}

impl<W> HtmlJsonEscapingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            pending_lt: false,
        }
    }
}

impl<W: Write> HtmlJsonEscapingWriter<W> {
    fn finish(mut self) -> std::io::Result<()> {
        if self.pending_lt {
            self.inner.write_all(b"<")?;
            self.pending_lt = false;
        }
        self.inner.flush()
    }
}

impl<W: Write> Write for HtmlJsonEscapingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &byte in buf {
            if self.pending_lt {
                if byte == b'/' {
                    self.inner.write_all(b"<\\/")?;
                } else {
                    self.inner.write_all(b"<")?;
                    if byte == b'<' {
                        self.pending_lt = true;
                        continue;
                    }
                    self.inner.write_all(&[byte])?;
                }
                self.pending_lt = false;
            } else if byte == b'<' {
                self.pending_lt = true;
            } else {
                self.inner.write_all(&[byte])?;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::core::provenance::Provenance;
    use crate::structure::graph::Epistemic;

    use super::super::{GraphCounts, GraphDegree, GraphExportNode};

    #[test]
    fn html_json_escaping_handles_split_script_close() {
        let mut output = Vec::new();
        {
            let mut writer = HtmlJsonEscapingWriter::new(&mut output);
            writer.write_all(b"<").unwrap();
            writer.write_all(b"/script>").unwrap();
            writer.finish().unwrap();
        }

        assert_eq!(String::from_utf8(output).unwrap(), "<\\/script>");
    }

    #[test]
    fn html_json_escaping_flushes_trailing_angle_bracket() {
        let mut output = Vec::new();
        {
            let mut writer = HtmlJsonEscapingWriter::new(&mut output);
            writer.write_all(b"value<").unwrap();
            writer.finish().unwrap();
        }

        assert_eq!(String::from_utf8(output).unwrap(), "value<");
    }

    #[test]
    fn graph_html_writer_streams_json_payload() {
        const NODE_COUNT: usize = 200;
        const SINGLE_WRITE_BUDGET: usize = 64 * 1024;

        let nodes = (0..NODE_COUNT)
            .map(|index| GraphExportNode {
                id: format!("file:{index}"),
                node_type: "file",
                label: "large-label-fragment".repeat(80),
                path: Some(format!("src/file_{index}.rs")),
                root_id: Some("primary".to_string()),
                file_id: None,
                language: Some("rust".to_string()),
                symbol_kind: None,
                visibility: None,
                degree: GraphDegree::default(),
                epistemic: Epistemic::ParserObserved,
                provenance: Provenance::structural("test", "rev", Vec::new()),
                metadata: serde_json::json!({
                    "payload": "large-metadata-fragment".repeat(80),
                }),
            })
            .collect::<Vec<_>>();
        let graph = GraphExport {
            generated_note: "test",
            schema_version: 1,
            graph_schema_version: 1,
            budget: "normal",
            counts: GraphCounts {
                nodes: NODE_COUNT,
                edges: 0,
                files: NODE_COUNT,
                symbols: 0,
                concepts: 0,
                edges_by_kind: BTreeMap::new(),
            },
            nodes,
            edges: Vec::new(),
        };

        let mut writer = CountingWriter::new();
        write_graph_html_to_writer(&mut writer, &graph).unwrap();

        assert!(writer.total_bytes > SINGLE_WRITE_BUDGET);
        assert!(
            writer.peak_single_write <= SINGLE_WRITE_BUDGET,
            "graph HTML writer buffered too much JSON: peak={} total={}",
            writer.peak_single_write,
            writer.total_bytes
        );
    }

    struct CountingWriter {
        peak_single_write: usize,
        total_bytes: usize,
    }

    impl CountingWriter {
        fn new() -> Self {
            Self {
                peak_single_write: 0,
                total_bytes: 0,
            }
        }
    }

    impl std::io::Write for CountingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.total_bytes += buf.len();
            self.peak_single_write = self.peak_single_write.max(buf.len());
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
