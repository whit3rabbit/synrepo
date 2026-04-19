//! Process-global access to the current in-memory graph snapshot.

use std::sync::{Arc, LazyLock};

use arc_swap::ArcSwap;

use super::Graph;

/// Atomic handle to the current in-memory graph snapshot.
pub static GRAPH_SNAPSHOT: LazyLock<ArcSwap<Graph>> =
    LazyLock::new(|| ArcSwap::from_pointee(Graph::empty()));

/// Load the current graph snapshot.
pub fn current() -> Arc<Graph> {
    GRAPH_SNAPSHOT.load_full()
}

/// Publish a fully-built graph snapshot atomically.
pub fn publish(new: Graph) {
    GRAPH_SNAPSHOT.store(Arc::new(new));
}

#[cfg(test)]
mod tests {
    use super::{current, publish};
    use crate::structure::graph::Graph;

    #[test]
    fn publish_replaces_the_current_graph() {
        let mut first = Graph::empty();
        first.snapshot_epoch = 1;
        publish(first);
        assert_eq!(current().snapshot_epoch, 1);

        let mut second = Graph::empty();
        second.snapshot_epoch = 2;
        publish(second);
        assert_eq!(current().snapshot_epoch, 2);
    }
}
