//! Data types for commentary work planning.

use std::path::PathBuf;

use crate::core::ids::{FileNodeId, NodeId};
use crate::pipeline::explain::CommentarySkipReason;

/// Fixed phases for commentary work.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentaryWorkPhase {
    Refresh,
    Seed,
}

/// Planned commentary work item.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentaryWorkItem {
    pub node_id: NodeId,
    pub file_id: FileNodeId,
    pub phase: CommentaryWorkPhase,
    pub path: String,
    pub qualified_name: Option<String>,
}

/// Plan for one `synrepo explain` run.
#[allow(missing_docs)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommentaryWorkPlan {
    pub refresh: Vec<CommentaryWorkItem>,
    pub file_seeds: Vec<CommentaryWorkItem>,
    pub symbol_seed_candidates: Vec<CommentaryWorkItem>,
    pub scoped_files: usize,
    pub scoped_symbols: usize,
}

#[allow(missing_docs)]
impl CommentaryWorkPlan {
    pub fn refresh_count(&self) -> usize {
        self.refresh.len()
    }

    pub fn file_seed_count(&self) -> usize {
        self.file_seeds.len()
    }

    pub fn symbol_seed_candidate_count(&self) -> usize {
        self.symbol_seed_candidates.len()
    }

    pub fn max_target_count(&self) -> usize {
        self.refresh_count() + self.file_seed_count() + self.symbol_seed_candidate_count()
    }

    pub fn scoped_file_count(&self) -> usize {
        self.scoped_files
    }

    pub fn scoped_symbol_count(&self) -> usize {
        self.scoped_symbols
    }

    pub fn is_empty(&self) -> bool {
        self.max_target_count() == 0
    }
}

/// Structured progress emitted while commentary refresh runs.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommentaryProgressEvent {
    ScanProgress {
        files_scanned: usize,
        files_total: usize,
        symbols_scanned: usize,
        symbols_total: usize,
    },
    PlanReady {
        refresh: usize,
        file_seeds: usize,
        symbol_seed_candidates: usize,
        scoped_files: usize,
        scoped_symbols: usize,
        max_targets: usize,
    },
    TargetStarted {
        item: CommentaryWorkItem,
        current: usize,
    },
    TargetFinished {
        item: CommentaryWorkItem,
        current: usize,
        generated: bool,
        skip_reason: Option<CommentarySkipReason>,
        skip_message: Option<String>,
        retry_attempts: usize,
        queued_for_next_run: bool,
    },
    DocsDirCreated {
        path: PathBuf,
    },
    DocWritten {
        path: PathBuf,
    },
    DocDeleted {
        path: PathBuf,
    },
    IndexDirCreated {
        path: PathBuf,
    },
    IndexUpdated {
        path: PathBuf,
        touched_paths: usize,
    },
    IndexRebuilt {
        path: PathBuf,
        touched_paths: usize,
    },
    PhaseSummary {
        phase: CommentaryWorkPhase,
        attempted: usize,
        generated: usize,
        not_generated: usize,
    },
    RunSummary {
        refreshed: usize,
        seeded: usize,
        not_generated: usize,
        attempted: usize,
        stopped: bool,
        queued_for_next_run: usize,
        skip_reasons: Vec<(String, usize)>,
    },
}
