// daemon.rs
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;

use crate::{collectors, server, state};

pub async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let state: state::SharedState = Arc::new(RwLock::new(state::GlobalState::default()));

    let s = state.clone();
    spawn_collector(move || collectors::docker::run(s.clone()));

    let s = state.clone();
    spawn_collector(move || collectors::ports::run(s.clone()));

    let s = state.clone();
    spawn_collector(move || collectors::ollama::run(s.clone()));

    let s = state.clone();
    spawn_collector(move || collectors::services::run(s.clone()));

    let s = state.clone();
    spawn_collector(move || collectors::ram::run(s.clone()));

    server::run(state).await
}

/// Runs `factory()` in a loop, restarting it if it panics.
///
/// Every collector's `run()` has an infinite loop inside it, so under normal
/// operation the future never resolves — the only way this loop body gets
/// re-entered is if the collector *panics*. The previous implementation did
/// `factory().await` directly inside this `loop`, which meant a panic would
/// unwind straight through this function's own task and kill it silently —
/// the "restarting in 1s" log line was dead code, never reachable.
///
/// Fix: run each collector as its *own* `tokio::spawn`'d task and `.await`
/// the `JoinHandle`. A panic inside that inner task is caught by tokio and
/// surfaced here as `Err(JoinError)` instead of unwinding into this loop, so
/// we can actually log it and restart.
fn spawn_collector<F, Fut>(factory: F)
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            match tokio::spawn(factory()).await {
                Ok(()) => {
                    // factory() returned normally (shouldn't happen for the
                    // current collectors, but handled in case one is added
                    // that legitimately exits instead of looping forever).
                    tracing::warn!("[tmuxd] collector exited, restarting in 1s...");
                }
                Err(join_err) if join_err.is_panic() => {
                    tracing::error!("[tmuxd] collector panicked, restarting in 1s...");
                }
                Err(join_err) => {
                    tracing::warn!("[tmuxd] collector task cancelled ({join_err}), restarting in 1s...");
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
}
