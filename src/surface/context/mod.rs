//! Task-context planning for high-level MCP asks.
//!
//! This layer does not compile graph facts itself. It turns an agent's
//! task-shaped request into bounded artifact targets that existing card and
//! context-pack compilers can serve deterministically.

/// Deterministic planner that maps asks to context-pack targets.
pub mod compiler;
/// Built-in task-context recipe names and inference.
pub mod recipe;
/// Request, grounding, confidence, budget, and target types.
pub mod types;

pub use compiler::compile_context_request;
pub use recipe::ContextRecipe;
pub use types::{
    Confidence, ContextAskRequest, ContextBudget, ContextScope, ContextShape, ContextTarget,
    GroundingMode, GroundingOptions,
};
