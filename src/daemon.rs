// Core entry point initialization loop spawning all state sub-collectors.

use std::sync::Arc;
use anyhow::Result;
use tokio::sync::RwLock;
use crate::{collectors, server, state};

// ── Daemon Runtime Initialization ──

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

// ── Fault-Tolerant Task Spawner ──

fn spawn_collector<F, Fut>(factory: F)
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            match tokio::spawn(factory()).await {
                Ok(()) => {
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