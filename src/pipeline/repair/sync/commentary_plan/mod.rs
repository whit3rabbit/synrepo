//! Commentary work planning: scan the graph + overlay to decide which nodes
//! need fresh commentary, and expose scope-prefix helpers reused by the CLI.

mod builder;
mod scope;
mod types;

pub use builder::load_commentary_work_plan;
pub use scope::{normalize_scope_prefixes, path_matches_any_prefix};
pub use types::{
    CommentaryProgressEvent, CommentaryWorkItem, CommentaryWorkPhase, CommentaryWorkPlan,
};

pub(crate) use builder::build_commentary_work_plan_with_progress;
