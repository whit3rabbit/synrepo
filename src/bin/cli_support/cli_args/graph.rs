//! Clap argument types for graph subcommands.

use clap::Subcommand;

use super::GraphDirectionArg;

#[derive(Subcommand)]
pub(crate) enum GraphCommand {
    /// Run a narrow traversal query against the graph store.
    Query {
        /// Query syntax: `<direction> <target> [edge_kind]`.
        /// `<target>` accepts a file path, qualified symbol name, or node ID.
        q: String,
    },

    /// Print graph statistics (node count by type, edge count by kind).
    Stats,

    /// Open a bounded terminal graph view or emit its JSON model.
    View {
        /// Optional target: node ID, file path, qualified symbol name, or short symbol name.
        target: Option<String>,
        /// Traversal direction.
        #[arg(long, value_enum, default_value = "both")]
        direction: GraphDirectionArg,
        /// Edge kind filter. Repeat to include multiple kinds.
        #[arg(long = "edge-kind")]
        edge_kind: Vec<String>,
        /// Traversal depth. Defaults to 1 and is clamped by the model.
        #[arg(long, default_value_t = 1)]
        depth: usize,
        /// Node and edge limit. Defaults to 100 and is clamped by the model.
        #[arg(long, default_value_t = 100)]
        limit: usize,
        /// Emit the bounded graph model as JSON instead of opening the TUI.
        #[arg(long)]
        json: bool,
    },
}
