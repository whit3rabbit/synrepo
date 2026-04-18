#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

// Release-gate soak suite.
//
// Run serially because these tests intentionally exercise real subprocess
// crashes, daemon lifecycles, and writer-lock contention:
//
// cargo test --test mutation_soak -- --ignored --test-threads=1

#[cfg(unix)]
#[path = "mutation_soak/support.rs"]
mod support;

#[cfg(unix)]
#[path = "mutation_soak/links_accept.rs"]
mod links_accept;

#[cfg(unix)]
#[path = "mutation_soak/watch_active.rs"]
mod watch_active;

#[cfg(unix)]
#[path = "mutation_soak/daemon_cleanup.rs"]
mod daemon_cleanup;

#[cfg(unix)]
#[path = "mutation_soak/writer_lock.rs"]
mod writer_lock;
