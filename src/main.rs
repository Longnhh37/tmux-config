// Binary orchestrator checking arguments and launching either the client query or background daemon.

mod client;
mod utils;
mod collectors;
mod daemon;
mod git;
mod render;
mod server;
mod state;

use anyhow::Result;

// ── Main Guard Routing ──

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);

    match args.next().as_deref() {
        Some("daemon") => daemon::run().await,

        Some("status") => {
            let pane_id   = args.next().unwrap_or_default();
            let pane_path = args.next().unwrap_or_else(|| ".".into());
            client::query(&pane_id, &pane_path).await
        }

        _ => {
            eprintln!("Usage:");
            eprintln!("  tmuxd daemon                        # start background daemon");
            eprintln!("  tmuxd status <pane_id> <pane_path>  # query daemon (used by tmux)");
            std::process::exit(1);
        }
    }
}