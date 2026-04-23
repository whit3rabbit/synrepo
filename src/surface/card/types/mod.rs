//! Card type definitions for the surface layer.
//!
//! This module re-exports all card types to maintain backward compatibility
//! with code that imports from `super::types` or `crate::surface::card::types`.

pub mod call_path;
pub mod change_risk;
pub mod entry_point;
pub mod file;
pub mod module;
pub mod public_api;
pub mod refs;
pub mod symbol;
pub mod test_surface;

// Re-export all types for backward compatibility.
pub use call_path::{CallPath, CallPathCard, CallPathEdge};
pub use change_risk::{ChangeRiskCard, RiskFactor, RiskLevel};
pub use entry_point::{EntryPoint, EntryPointCard, EntryPointKind};
pub use file::FileCard;
pub use module::ModuleCard;
pub use public_api::{PublicAPICard, PublicAPIEntry};
pub use refs::{FileRef, SymbolRef};
pub use symbol::{Freshness, OverlayCommentary, ProposedLink, SymbolCard};
pub use test_surface::{TestAssociation, TestEntry, TestSurfaceCard};

// Re-export SourceStore from sibling module.
pub use super::{ContextAccounting, SourceStore};
