use std::{path::PathBuf, sync::mpsc, time::Duration};

use crate::{pipeline::repair::SyncOptions, pipeline::watch::control::WatchControlResponse};

pub(super) enum LoopMessage {
    Stop,
    ReconcileNow {
        respond_to: mpsc::Sender<WatchControlResponse>,
        fast: bool,
    },
    SyncNow {
        respond_to: mpsc::Sender<WatchControlResponse>,
        options: SyncOptions,
    },
    EmbeddingsBuildNow {
        respond_to: mpsc::Sender<WatchControlResponse>,
    },
    SuppressPaths {
        respond_to: mpsc::Sender<WatchControlResponse>,
        paths: Vec<PathBuf>,
        ttl: Duration,
    },
}
