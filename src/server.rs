// server.rs
//
// Request/response flow:
//
//   tmux  ──►  tmuxd status <pane_id> <pane_current_path>
//                │
//                ▼  (Unix socket)
//   server::run ──►  parse pane_id + pane_path
//                      │
//                      ├── git::get_cached(pane_path)   ← per-pane
//                      │
//                      └── render::build(global_state, per_pane_ctx)
//                               │
//                               ├── GlobalState  (docker / ports / ollama / k8s / brew / ram)  ← persistent
//                               └── PerPaneContext (path + git)                                ← per-pane

use crate::{git, render, state, utils};
use anyhow::Result;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

/// Accept connections on the Unix socket.
///
/// Protocol (text, newline-terminated):
///   Request  →  "<pane_id> <pane_path>\n"
///   Response ←  "<rendered status string>"
pub async fn run(state: state::SharedState) -> Result<()> {
    let sock_path = utils::sock_path();

    // Bind first instead of connect-then-bind: a plain connect-first check has a
    // race window between the check and the bind where two daemons starting at
    // once could both "win". Binding is the actual point of contention, so we
    // let the OS arbitrate it and only fall back to a liveness probe if the
    // socket file was already there.
    let listener = match UnixListener::bind(&sock_path) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if tokio::net::UnixStream::connect(&sock_path).await.is_ok() {
                eprintln!("[tmuxd] Another daemon instance is already running.");
                std::process::exit(1);
            }
            // Nothing answered — stale socket file left over from a crash/kill.
            let _ = std::fs::remove_file(&sock_path);
            match UnixListener::bind(&sock_path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[tmuxd] failed to bind socket: {e}");
                    return Err(e.into());
                }
            }
        }
        Err(e) => {
            eprintln!("[tmuxd] failed to bind socket: {e}");
            return Err(e.into());
        }
    };

    eprintln!("[tmuxd] listening on {}", sock_path.display());
    spawn_shutdown_cleanup(sock_path.clone());

    loop {
        let (stream, _) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();

            let Ok(Some(line)) = lines.next_line().await else {
                return;
            };

            // Parse "<pane_id> <pane_path>" — splitn(2) gives at most 2 parts.
            let mut parts = line.splitn(2, ' ');
            let pane_id = match parts.next() {
                Some(id) if !id.is_empty() => id.to_string(),
                _ => return,
            };
            let pane_path = parts.next().unwrap_or(".").to_string();

            // ── Per-pane context ─────────────────────────────────────────────
            // git is fetched per pane_path (stale-while-revalidate).
            // Changes automatically when you switch to a pane in a different dir.
            let git_info = git::get_cached(&pane_path).await;
            let pane_ctx = state::PerPaneContext {
                pane_path: pane_path.clone(),
                git: git_info,
            };

            // ── Global state (docker / ports / ollama) ───────────────────────
            // Passed by reference — same values seen by every pane.
            let response = render::build(&state, &pane_ctx).await;

            // Write cache file so the client can show stale data on daemon restart.
            let cache_path = utils::cache_path(&pane_id);
            let _ = tokio::fs::write(&cache_path, &response).await;

            let _ = writer.write_all(response.as_bytes()).await;
        });
    }
}

/// Remove the socket file on a clean shutdown (Ctrl+C or SIGTERM) instead of
/// leaving it behind for the next `run()` to discover as "stale" on startup.
fn spawn_shutdown_cleanup(sock_path: std::path::PathBuf) {
    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();

        #[cfg(unix)]
        {
            let mut term = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[tmuxd] failed to install SIGTERM handler: {e}");
                    let _ = ctrl_c.await;
                    let _ = std::fs::remove_file(&sock_path);
                    std::process::exit(0);
                }
            };
            tokio::select! {
                _ = ctrl_c => {},
                _ = term.recv() => {},
            }
        }
        #[cfg(not(unix))]
        {
            let _ = ctrl_c.await;
        }

        eprintln!("[tmuxd] shutting down, removing {}", sock_path.display());
        let _ = std::fs::remove_file(&sock_path);
        std::process::exit(0);
    });
}
