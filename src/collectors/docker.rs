use crate::state::SharedState;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

const RETRY_DELAY: Duration = Duration::from_secs(2);

pub async fn run(state: SharedState) {
    let mut cached_host: Option<String> = None;

    loop {
        let host = if let Some(ref h) = cached_host {
            let path = h.trim_start_matches("unix://");
            if tokio::fs::metadata(path).await.is_ok() {
                Some(h.clone())
            } else {
                resolve_docker_host().await
            }
        } else {
            resolve_docker_host().await
        };

        match host {
            Some(h) => {
                cached_host = Some(h.clone());

                state.write().await.docker_count = count_containers(&h).await;
                stream_events(&h, state.clone()).await;

                tokio::time::sleep(RETRY_DELAY).await;
            }
            None => {
                cached_host = None;
                if state.read().await.docker_count != 0 {
                    state.write().await.docker_count = 0;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn stream_events(host: &str, state: SharedState) {
    let path = host.trim_start_matches("unix://");

    let mut stream = match UnixStream::connect(path).await {
        Ok(s) => s,
        Err(_) => return,
    };

    let req = "GET /events?filters=%7B%22type%22%3A%7B%22container%22%3Atrue%7D%2C%22event%22%3A%7B%22start%22%3Atrue%2C%22die%22%3Atrue%2C%22stop%22%3Atrue%2C%22pause%22%3Atrue%2C%22unpause%22%3Atrue%7D%7D HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    if stream.write_all(req.as_bytes()).await.is_err() {
        return;
    }

    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    // Bỏ qua các HTTP headers
    loop {
        line.clear();
        if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
        }
        if line == "\r\n" {
            break;
        }
    }

    loop {
        line.clear();
        if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            let count = count_containers(host).await;
            state.write().await.docker_count = count;
        }
    }
}

async fn count_containers(host: &str) -> u32 {
    let path = host.trim_start_matches("unix://");
    let mut stream = match UnixStream::connect(path).await {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let req = "GET /containers/json HTTP/1.0\r\nHost: localhost\r\n\r\n";
    if stream.write_all(req.as_bytes()).await.is_err() {
        return 0;
    }

    let mut resp = String::new();
    if stream.read_to_string(&mut resp).await.is_err() {
        return 0;
    }

    if let Some(body_start) = resp.find("\r\n\r\n") {
        let body = &resp[body_start + 4..];
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(arr) = json.as_array() {
                return arr.len() as u32;
            }
        }
    }
    0
}

async fn resolve_docker_host() -> Option<String> {
    let home = crate::utils::home_dir();
    let candidates = [
        std::env::var("DOCKER_HOST").ok(),
        Some(format!("unix://{home}/.orbstack/run/docker.sock")),
        Some("unix:///var/run/docker.sock".into()),
    ];

    for candidate in candidates.into_iter().flatten() {
        let path = candidate.trim_start_matches("unix://");
        if tokio::fs::metadata(path).await.is_ok() {
            return Some(candidate);
        }
    }
    None
}
