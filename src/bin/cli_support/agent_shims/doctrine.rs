//! Re-export of the canonical agent-doctrine macros and constants.
//!
//! The source-of-truth lives in the library crate at
//! `synrepo::surface::agent_doctrine`. Keeping a thin re-export here lets the
//! binary's shim constants resolve `doctrine_block!()` at compile time without
//! threading the library path through every `concat!` site.

// `#[macro_export]` places the macro at the root of the library crate, so
// binary-side call sites reach it via `synrepo::doctrine_block!()`.
pub(crate) use synrepo::doctrine_block;

// Consumed by tests that enforce byte-identical doctrine inclusion across
// shims; the shim constants themselves use the `doctrine_block!()` macro to
// embed this text at compile time, not this const, so outside of tests the
// symbol looks dead.
#[allow(dead_code)]
pub(crate) const DOCTRINE_BLOCK: &str = synrepo::surface::agent_doctrine::DOCTRINE_BLOCK;

// Available if/when the binary starts surfacing the escalation sentence
// directly (for example in bootstrap report copy).
#[allow(dead_code)]
pub(crate) const TOOL_DESC_ESCALATION_LINE: &str =
    synrepo::surface::agent_doctrine::TOOL_DESC_ESCALATION_LINE;
