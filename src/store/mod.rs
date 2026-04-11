//! Graph and overlay store backends.
//!
//! Phase 1 ships a sqlite-backed implementation of [`crate::structure::graph::GraphStore`].
//! An in-memory test store will live next to it for unit tests.
//! The overlay store backend ships in phase 4.

pub mod compatibility;
pub mod sqlite;
