mod agent_notes;
mod commentary;
mod cross_links;
mod declared_links;
mod drift;
mod export_surface;
mod legacy_installs;
mod rationale;
mod store_maintenance;
mod structural_refresh;
mod writer_lock;

// Re-imported so child submodules can refer to these via `super::` instead
// of `super::super::`. The names are not used directly in this file; their
// purpose is to seed the surfaces module's namespace for the children.
use super::{RepairContext, SurfaceCheck};

pub use agent_notes::AgentNotesOverlayCheck;
pub use commentary::{scan_commentary_staleness, CommentaryOverlayCheck, CommentaryScan};
pub use cross_links::ProposedLinksOverlayCheck;
pub use declared_links::DeclaredLinksCheck;
pub use drift::{EdgeDriftCheck, RetiredObservationsCheck};
pub use export_surface::ExportSurfaceCheck;
pub use legacy_installs::LegacyAgentInstallsCheck;
pub use rationale::StaleRationaleCheck;
pub use store_maintenance::StoreMaintenanceCheck;
pub use structural_refresh::StructuralRefreshCheck;
pub use writer_lock::WriterLockCheck;
