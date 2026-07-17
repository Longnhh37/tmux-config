use crate::{git, render, state, utils};
use anyhow::Result;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    signal,
};

pub async fn run(state: state::SharedState) -> Result<()> {
    let sock_path = utils::sock_path();

    let listener = match UnixListener::bind(&sock_path) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if UnixStream::connect(&sock_path).await.is_ok() {
                eprintln!("[tmuxd] Another daemon instance is already running.");
                std::process::exit(1);
            }
            let _ = std::fs::remove_file(&sock_path);
            UnixListener::bind(&sock_path)?
        }
        Err(e) => return Err(e.into()),
    };

    eprintln!("[tmuxd] listening on {}", sock_path.display());

    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
    let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;

    loop {
        tokio::select! {
            accept_res = listener.accept() => {
                let (stream, _) = match accept_res {
                    Ok(res) => res,
                    Err(_) => continue,
                };
                let state = state.clone();

                tokio::spawn(async move {
                    let (reader, mut writer) = stream.into_split();
                    let mut lines = BufReader::new(reader).lines();

                    let Ok(Some(line)) = lines.next_line().await else {
                        return;
                    };

                    let mut parts = line.splitn(2, ' ');
                    let pane_id = match parts.next() {
                        Some(id) if !id.is_empty() => id.to_string(),
                        _ => return,
                    };
                    let pane_path = parts.next().unwrap_or(".").to_string();

                    let git_info = git::get_cached(&pane_path).await;
                    let pane_ctx = state::PerPaneContext {
                        pane_path: pane_path.clone(),
                        git: git_info,
                    };

                    let response = render::build(&state, &pane_ctx).await;

                    let cache_path = utils::cache_path(&pane_id);
                    let _ = tokio::fs::write(&cache_path, &response).await;

                    let _ = writer.write_all(response.as_bytes()).await;
                });
            }
            _ = sigint.recv() => break,
            _ = sigterm.recv() => break,
        }
    }

    let _ = std::fs::remove_file(&sock_path);
    eprintln!("[tmuxd] shutting down cleanly.");
    Ok(())
}
