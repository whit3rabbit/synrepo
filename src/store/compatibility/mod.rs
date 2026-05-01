//! Runtime storage compatibility policy for `.synrepo/`.

mod evaluate;
mod snapshot;
mod types;

// Constants shared across sub-modules. Private here; child modules
// access them via `super::CONST`.
const SNAPSHOT_VERSION: u32 = 1;
/// Format version for the graph store (root-discriminated files).
pub const GRAPH_FORMAT_VERSION: u32 = 2;
/// Default format version for non-graph stores.
pub const DEFAULT_FORMAT_VERSION: u32 = 1;
const SNAPSHOT_FILENAME: &str = "storage-compat.json";

pub(crate) use evaluate::clear_store_contents;
pub use evaluate::{apply_runtime_actions, clear_blocked_stores, evaluate_runtime};
pub use snapshot::{ensure_runtime_layout, snapshot_path, write_runtime_snapshot};
pub use types::{
    CompatAction, CompatibilityEntry, CompatibilityReport, ConfigFingerprints,
    RuntimeCompatibilitySnapshot, StoreClass, StoreId,
};
