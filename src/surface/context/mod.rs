//! Task-context planning for high-level MCP asks.
//!
//! This layer does not compile graph facts itself. It turns an agent's
//! task-shaped request into bounded artifact targets that existing card and
//! context-pack compilers can serve deterministically.

pub mod compiler;
pub mod recipe;
pub mod types;

pub use compiler::compile_context_request;
pub use recipe::ContextRecipe;
pub use types::{
    Confidence, ContextAskRequest, ContextBudget, ContextScope, ContextShape, ContextTarget,
    GroundingMode, GroundingOptions,
};
