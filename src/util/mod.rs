//! Small cross-cutting utilities shared across layers.
//!
//! Helpers here must be trivially reusable from both the library and binary
//! crates and must not depend on any other synrepo module. Anything bigger
//! than a one-file helper belongs in its own top-level module instead.

pub mod atomic_write;

pub use atomic_write::atomic_write;
