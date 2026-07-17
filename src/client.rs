use anyhow::Result;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

/// Connect to the daemon socket, send the request, and print the response.
///
/// Falls back in order:
///   1. Daemon socket → fresh render (< 5 ms)
///   2. Cache file    → last known value (stale but instant)
///   3. Empty string  → better than hanging tmux
pub async fn query(pane_id: &str, pane_path: &str) -> Result<()> {
    match try_socket(pane_id, pane_path).await {
        Ok(s) => print!("{s}"),
        Err(_) => {
            let cache = crate::utils::cache_path(pane_id);
            if let Ok(cached) = tokio::fs::read_to_string(&cache).await {
                print!("{cached}");
            }
        }
    }
    Ok(())
}

async fn try_socket(pane_id: &str, pane_path: &str) -> anyhow::Result<String> {
    // Hard timeout: if the daemon doesn't answer in 200 ms we'd rather show
    // stale data than block the tmux status bar.
    let connect = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        UnixStream::connect(crate::utils::sock_path()),
    )
    .await??;

    let mut stream = connect;
    let req = format!("{pane_id} {pane_path}\n");
    stream.write_all(req.as_bytes()).await?;
    // Signal EOF so the server knows the request is complete.
    stream.shutdown().await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    Ok(response)
}
