//! Self-contained HTML renderer for canonical graph exports.

mod script;
mod shell;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

const GRAPH_HTML_FILENAME: &str = "graph.html";

pub(super) fn write_graph_html_file<F>(export_dir: &Path, write_payload: F) -> crate::Result<()>
where
    F: FnOnce(&mut dyn Write) -> crate::Result<()>,
{
    let mut writer = BufWriter::new(File::create(export_dir.join(GRAPH_HTML_FILENAME))?);
    write_graph_html_to_writer(&mut writer, write_payload)?;
    writer.flush()?;
    Ok(())
}

fn write_graph_html_to_writer<W, F>(writer: &mut W, write_payload: F) -> crate::Result<()>
where
    W: Write,
    F: FnOnce(&mut dyn Write) -> crate::Result<()>,
{
    writer.write_all(shell::HTML_PREFIX.as_bytes())?;
    {
        let mut escaping = HtmlJsonEscapingWriter::new(&mut *writer);
        write_payload(&mut escaping)?;
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

    use serde::Serialize;

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

        let mut writer = CountingWriter::new();
        write_graph_html_to_writer(&mut writer, |payload_writer| {
            serde_json::to_writer(payload_writer, &LargePayload { nodes: NODE_COUNT }).map_err(
                |e| crate::Error::Other(anyhow::anyhow!("graph HTML data serialize failed: {e}")),
            )
        })
        .unwrap();

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

    struct LargePayload {
        nodes: usize,
    }

    impl Serialize for LargePayload {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            use serde::ser::SerializeStruct;

            let mut state = serializer.serialize_struct("LargePayload", 1)?;
            state.serialize_field("nodes", &LargeNodes { count: self.nodes })?;
            state.end()
        }
    }

    struct LargeNodes {
        count: usize,
    }

    impl Serialize for LargeNodes {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            use serde::ser::SerializeSeq;

            let mut seq = serializer.serialize_seq(Some(self.count))?;
            for index in 0..self.count {
                seq.serialize_element(&serde_json::json!({
                    "id": format!("file:{index}"),
                    "label": "large-label-fragment".repeat(80),
                    "metadata": {
                        "payload": "large-metadata-fragment".repeat(80),
                    },
                }))?;
            }
            seq.end()
        }
    }
}
