use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tempfile::{tempdir, TempDir};
use tokio::sync::Semaphore;

use super::*;
use crate::cli_support::commands::mcp_runtime::prepare_state;
use synrepo::bootstrap::bootstrap;
use synrepo::config::test_home;

struct HomeFixture {
    _lock: synrepo::test_support::GlobalTestLock,
    _home: TempDir,
    _guard: test_home::HomeEnvGuard,
}

fn home_fixture() -> HomeFixture {
    let lock = synrepo::test_support::global_test_lock(test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let guard = test_home::HomeEnvGuard::redirect_to(home.path());
    HomeFixture {
        _lock: lock,
        _home: home,
        _guard: guard,
    }
}

fn ready_repo(body: &str) -> (TempDir, std::path::PathBuf) {
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), body).unwrap();
    bootstrap(dir.path(), None, false).unwrap();
    let path = dir.path().to_path_buf();
    (dir, path)
}

#[tokio::test]
async fn acquire_permit_saturation_returns_busy() {
    let semaphore = Semaphore::new(1);
    let _permit = semaphore.acquire().await.unwrap();
    let res = acquire_permit_from_semaphore(&semaphore, Duration::from_millis(20)).await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code().as_str(), "BUSY");
}

#[tokio::test]
async fn acquire_blocking_permit_saturation_returns_busy() {
    let semaphore = Arc::new(Semaphore::new(1));
    let _permit = semaphore.clone().acquire_owned().await.unwrap();
    let res = acquire_blocking_permit(&semaphore, Duration::from_millis(20)).await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code().as_str(), "BUSY");
}

#[test]
fn blocking_tool_timeout_retains_permit_while_worker_runs() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn latched_worker_needle() {}\n");
    let state = prepare_state(&repo_path).unwrap();
    let server = SynrepoServer::new_optional_with_timeout(
        Some(state),
        false,
        true,
        Duration::from_millis(15),
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let semaphore = Arc::new(Semaphore::new(1));

    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));

    let entered_c = Arc::clone(&entered);
    let release_c = Arc::clone(&release);
    let finished_c = Arc::clone(&finished);

    let server_c = server.clone();
    let sem_c = Arc::clone(&semaphore);

    // Call tool that latches inside the blocking worker
    let handle = runtime.spawn(async move {
        server_c
            .with_tool_state_blocking_with_permit("synrepo_overview", None, &sem_c, move |_state| {
                entered_c.store(true, Ordering::SeqCst);
                while !release_c.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                finished_c.store(true, Ordering::SeqCst);
                "{}".to_string()
            })
            .await
    });

    let output = runtime.block_on(handle).unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    // 1. Tool call timed out and returned TIMEOUT error
    assert_eq!(value["error"]["code"], "TIMEOUT");

    // 2. The worker closure is still running, so permit is still held
    assert_eq!(semaphore.available_permits(), 0);

    // 3. A concurrent call attempting to acquire a permit gets BUSY
    let busy_output = runtime.block_on(server.with_tool_state_blocking_with_permit(
        "synrepo_overview",
        None,
        &semaphore,
        |_| "{}".to_string(),
    ));
    let busy_value: serde_json::Value = serde_json::from_str(&busy_output).unwrap();
    assert_eq!(busy_value["error"]["code"], "BUSY");

    // 4. Release the latched worker
    release.store(true, Ordering::SeqCst);

    // Wait for worker to finish and drop permit
    let mut waited = 0;
    while !finished.load(Ordering::SeqCst) && waited < 100 {
        std::thread::sleep(Duration::from_millis(10));
        waited += 1;
    }
    assert!(finished.load(Ordering::SeqCst));

    // Permit is restored
    assert_eq!(semaphore.available_permits(), 1);
}
