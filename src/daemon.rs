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

fn spawn_collector<F, Fut>(factory: F)
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    tokio::spawn(async move {
        loop {
            factory().await;
            tracing::warn!("[tmuxd] collector crashed, restarting in 1s...");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
}
