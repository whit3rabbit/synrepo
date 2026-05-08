/// Reason for reconcile staleness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReconcileStaleness {
    /// The last reconcile completed successfully, but it occurred too long ago.
    Age {
        /// RFC 3339 UTC timestamp of the last reconcile.
        last_reconcile_at: String,
    },
    /// The last reconcile did not complete, or the outcome was not "completed".
    Outcome(String),
}

/// How fresh the last reconcile appears based on its recorded outcome and timestamp.
///
/// Staleness is determined by either a non-completed outcome or an age
/// exceeding RECONCILE_STALENESS_THRESHOLD_SECONDS.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReconcileHealth {
    /// The last reconcile completed successfully and recently.
    Current,
    /// The last reconcile is stale.
    Stale(ReconcileStaleness),
    /// The watch service holds the lease but its last reconcile is older than
    /// the staleness threshold. Distinct from `Stale` because the watch
    /// service is responsible for keeping the graph current; an old timestamp
    /// while watch is up signals a wedged loop, not idle staleness. Mitigation
    /// is to restart watch, not run a manual reconcile.
    WatchStalled {
        /// RFC 3339 UTC timestamp of the last reconcile recorded by watch.
        last_reconcile_at: String,
    },
    /// No reconcile state file exists; the system has never reconciled.
    Unknown,
    /// The reconcile state file exists but is malformed.
    Corrupt(String),
}

/// Current writer lock ownership status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WriterStatus {
    /// No writer lock is held.
    Free,
    /// The lock is held by the current process.
    HeldBySelf,
    /// The lock file records a different process ID.
    ///
    /// Diagnostics reports this only when the kernel advisory lock is also
    /// held; stale metadata without a flock is treated as free.
    HeldByOther {
        /// PID recorded in the lock file.
        pid: u32,
    },
    /// The lock file exists but is unreadable or malformed.
    Corrupt(String),
}

/// Embedding subsystem health.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmbeddingHealth {
    /// Semantic triage is not enabled.
    Disabled,
    /// The embedding index and model cache are available.
    Available {
        /// Configured provider name.
        provider: String,
        /// Whether the provider was explicit or defaulted from legacy config.
        provider_source: crate::config::SemanticProviderSource,
        /// Configured model name.
        model: String,
        /// Embedding vector dimension.
        dim: u16,
        /// Number of chunks in the index.
        chunks: usize,
    },
    /// The index or model is missing or corrupted.
    Degraded {
        /// Configured provider name.
        provider: String,
        /// Whether the provider was explicit or defaulted from legacy config.
        provider_source: crate::config::SemanticProviderSource,
        /// Human-readable degraded reason.
        reason: String,
    },
}

/// Top-level operational diagnostics for a `.synrepo/` runtime.
#[derive(Clone, Debug)]
pub struct RuntimeDiagnostics {
    /// Reconcile system health.
    pub reconcile_health: ReconcileHealth,
    /// Current watch-service status.
    pub watch_status: crate::pipeline::watch::WatchServiceStatus,
    /// Current writer lock status.
    pub writer_status: WriterStatus,
    /// Non-trivial storage compatibility guidance lines.
    pub store_guidance: Vec<String>,
    /// Raw reconcile state, if present.
    pub last_reconcile: Option<crate::pipeline::watch::ReconcileState>,
    /// Embedding subsystem health.
    pub embedding_health: EmbeddingHealth,
}

impl RuntimeDiagnostics {
    /// Render a human-readable diagnostic summary for CLI or logging output.
    pub fn render(&self) -> String {
        let mut out = String::new();

        out.push_str("Reconcile: ");
        match &self.reconcile_health {
            ReconcileHealth::Current => out.push_str("current\n"),
            ReconcileHealth::Stale(ReconcileStaleness::Outcome(last_outcome)) => {
                out.push_str(&format!("stale (last outcome: {last_outcome})\n"));
            }
            ReconcileHealth::Stale(ReconcileStaleness::Age { .. }) => {
                out.push_str("stale (over 1 hour old)\n");
            }
            ReconcileHealth::WatchStalled { last_reconcile_at } => {
                out.push_str(&format!(
                    "watch_stalled (watch is up but last reconcile {last_reconcile_at} is over 1 hour old)\n"
                ));
            }
            ReconcileHealth::Unknown => out.push_str("unknown (no reconcile state)\n"),
            ReconcileHealth::Corrupt(e) => out.push_str(&format!("corrupt ({e})\n")),
        }

        out.push_str("Writer: ");
        match &self.writer_status {
            WriterStatus::Free => out.push_str("free\n"),
            WriterStatus::HeldBySelf => out.push_str("held by current process\n"),
            WriterStatus::HeldByOther { pid } => out.push_str(&format!("held by pid {pid}\n")),
            WriterStatus::Corrupt(e) => out.push_str(&format!("corrupt ({e})\n")),
        }

        if let Some(state) = &self.last_reconcile {
            out.push_str(&format!(
                "Last reconcile: {} ({} events)\n",
                state.last_reconcile_at, state.triggering_events,
            ));
            if let (Some(files), Some(syms)) = (state.files_discovered, state.symbols_extracted) {
                out.push_str(&format!(
                    "  files_discovered={files}, symbols_extracted={syms}\n"
                ));
            }
        }

        for line in &self.store_guidance {
            out.push_str(&format!("Store: {line}\n"));
        }

        match &self.embedding_health {
            EmbeddingHealth::Disabled => {}
            EmbeddingHealth::Available {
                provider,
                provider_source,
                model,
                dim,
                chunks,
            } => {
                out.push_str(&format!(
                    "Embedding: available (provider={}, source={}, model={}, dim={}, chunks={})\n",
                    provider,
                    provider_source.as_str(),
                    model,
                    dim,
                    chunks
                ));
            }
            EmbeddingHealth::Degraded {
                provider,
                provider_source,
                reason,
            } => {
                out.push_str(&format!(
                    "Embedding: degraded (provider={}, source={}, reason={})\n",
                    provider,
                    provider_source.as_str(),
                    reason
                ));
            }
        }

        out
    }
}
