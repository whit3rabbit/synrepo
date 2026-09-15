use super::*;

fn active_scheduler(
    kind: WatchOperationKind,
    delay: Duration,
    respond_to: Option<mpsc::Sender<WatchControlResponse>>,
) -> WatchOperationScheduler {
    let handle = thread::spawn(move || {
        thread::sleep(delay);
        WatchOperationResult::AutoEmbeddings { result: Ok(()) }
    });
    WatchOperationScheduler {
        active: Some(ActiveOperation {
            kind,
            handle,
            respond_to,
        }),
    }
}

fn test_context() -> (tempfile::TempDir, WatchOperationContext) {
    let temp = tempfile::tempdir().unwrap();
    let synrepo_dir = temp.path().join(".synrepo");
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let state = super::super::lease::WatchDaemonState::new(
        &synrepo_dir,
        super::super::lease::WatchServiceMode::Foreground,
    );
    let handle = WatchStateHandle::new(
        super::super::lease::watch_daemon_state_path(&synrepo_dir),
        state,
    );
    let context = WatchOperationContext::new(
        temp.path().to_path_buf(),
        Config::default(),
        synrepo_dir,
        None,
        handle,
        Arc::new(AtomicBool::new(false)),
    );
    (temp, context)
}

#[test]
fn active_operation_rejects_every_other_mutation_kind() {
    let mut scheduler = active_scheduler(WatchOperationKind::Sync, Duration::from_millis(50), None);
    let (_temp, context) = test_context();

    let error = scheduler
        .start(
            context,
            WatchOperation::Embeddings {
                trigger: EmbeddingTrigger::Manual,
            },
            None,
        )
        .unwrap_err();

    assert!(error.contains("busy with sync"));
    scheduler.shutdown(false);
}

#[test]
fn completion_reports_worker_panics() {
    let handle = thread::spawn(|| panic!("boom"));
    let mut scheduler = WatchOperationScheduler {
        active: Some(ActiveOperation {
            kind: WatchOperationKind::Reconcile,
            handle,
            respond_to: None,
        }),
    };
    while !scheduler.active.as_ref().unwrap().handle.is_finished() {
        thread::yield_now();
    }

    let completion = scheduler.reap_finished().unwrap();

    let error = match completion.result {
        Ok(_) => panic!("panicking worker must report an error"),
        Err(error) => error,
    };
    assert!(error.contains("reconcile worker panicked"));
    assert!(scheduler.is_idle());
}

#[test]
fn stop_rejects_active_request_before_waiting_for_worker() {
    let (tx, rx) = mpsc::channel();
    let mut scheduler = active_scheduler(
        WatchOperationKind::Embeddings,
        Duration::from_millis(50),
        Some(tx),
    );

    scheduler.reject_active_request_on_stop();

    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        WatchControlResponse::Error { message } if message.contains("stopping")
    ));
    scheduler.shutdown(false);
}

#[test]
fn embedded_shutdown_waits_but_process_owned_shutdown_is_bounded() {
    let mut embedded = active_scheduler(
        WatchOperationKind::Reconcile,
        Duration::from_millis(40),
        None,
    );
    let started = Instant::now();
    embedded.shutdown_with_timeout(false, Duration::from_millis(1));
    assert!(started.elapsed() >= Duration::from_millis(30));

    let mut process_owned = active_scheduler(
        WatchOperationKind::Reconcile,
        Duration::from_millis(200),
        None,
    );
    let started = Instant::now();
    process_owned.shutdown_with_timeout(true, Duration::from_millis(10));
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn dropping_request_receiver_does_not_break_shutdown() {
    let (tx, rx) = mpsc::channel();
    drop(rx);
    let mut scheduler = active_scheduler(
        WatchOperationKind::Sync,
        Duration::from_millis(10),
        Some(tx),
    );

    scheduler.shutdown(false);

    assert!(scheduler.is_idle());
}
