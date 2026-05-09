use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use crate::config::Config;
use crate::structure::graph::{snapshot, with_graph_read_snapshot, Graph, GraphReader, GraphStore};

static SNAPSHOT_DISABLED_LOGGED: AtomicBool = AtomicBool::new(false);

pub fn run_graph_snapshot_commit(
    repo_root: &Path,
    config: &Config,
    graph: &dyn GraphStore,
    snapshot_epoch: u64,
) -> crate::Result<()> {
    if config.max_graph_snapshot_bytes == 0 {
        if !SNAPSHOT_DISABLED_LOGGED.swap(true, Ordering::Relaxed) {
            tracing::info!("graph snapshot publishing disabled by max_graph_snapshot_bytes = 0");
        }
        return Ok(());
    }

    let mut snapshot_graph = with_graph_read_snapshot(graph, Graph::from_store)?;
    snapshot_graph.snapshot_epoch = snapshot_epoch;
    snapshot_graph.published_at = SystemTime::now();

    let approx_bytes = snapshot_graph.approx_bytes();
    if approx_bytes > config.max_graph_snapshot_bytes {
        tracing::warn!(
            snapshot_epoch,
            approx_bytes,
            max_graph_snapshot_bytes = config.max_graph_snapshot_bytes,
            file_count = snapshot_graph.files.len(),
            symbol_count = snapshot_graph.symbols.len(),
            edge_count = snapshot_graph.all_edges()?.len(),
            "graph snapshot exceeds configured memory ceiling; skipping snapshot publication"
        );
        return Ok(());
    }

    snapshot::publish(repo_root, snapshot_graph);
    Ok(())
}
