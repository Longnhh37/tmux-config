use anyhow::Result;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};
use crate::utils;

pub async fn query(pane_id: &str, pane_path: &str) -> Result<()> {
    match try_socket(pane_id, pane_path).await {
        Ok(s) => print!("{s}"),
        Err(_) => {
            let cache = utils::cache_path(pane_id);
            if let Ok(cached) = tokio::fs::read_to_string(&cache).await {
                print!("{cached}");
            }
        }
    }
    Ok(())
}

async fn try_socket(pane_id: &str, pane_path: &str) -> anyhow::Result<String> {
    let connect = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        UnixStream::connect(utils::sock_path()),
    )
    .await??;

    let mut stream = connect;
    let req = format!("{pane_id} {pane_path}\n");
    stream.write_all(req.as_bytes()).await?;
    stream.shutdown().await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    Ok(response)
}
