//! Watch-owned embedding build and refresh helpers.

mod job;
mod scheduler;

#[cfg(test)]
mod tests;

pub(in crate::pipeline::watch) use job::{run_manual_embedding_build, EmbeddingJobContext};
pub(in crate::pipeline::watch) use scheduler::{
    EmbeddingRefreshScheduler, ReconcileEmbeddingObservation,
};
