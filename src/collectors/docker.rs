// collectors/docker.rs
use crate::state::SharedState;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

const RETRY_DELAY: Duration = Duration::from_secs(2);

pub async fn run(state: SharedState) {
    let mut cached_host: Option<String> = None;

    loop {
        // Nếu có cached host, quick-check socket còn sống không
        // thay vì probe toàn bộ candidates từ đầu
        let host = if let Some(ref h) = cached_host {
            let path = h.trim_start_matches("unix://");
            if tokio::fs::metadata(path).await.is_ok() {
                Some(h.clone()) // socket còn → dùng luôn
            } else {
                resolve_docker_host().await // socket mất → probe lại
            }
        } else {
            resolve_docker_host().await
        };

        match host {
            Some(h) => {
                cached_host = Some(h.clone()); // giữ cho vòng tiếp theo

                state.write().await.docker_count = count_containers(&h).await;
                stream_events(&h, state.clone()).await;

                // Stream kết thúc — KHÔNG reset cached_host
                // Vòng tiếp theo sẽ quick-check socket thay vì probe lại toàn bộ
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
    let mut child = match docker_cmd(host)
        .args([
            "events",
            "--filter",
            "type=container",
            "--filter",
            "event=start",
            "--filter",
            "event=die",
            "--filter",
            "event=stop",
            "--filter",
            "event=pause",
            "--filter",
            "event=unpause",
            "--format",
            "{{.Status}}",
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return,
    };

    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();

    while let Ok(Some(_event)) = lines.next_line().await {
        // Bất kỳ event nào → recount ngay lập tức
        let count = count_containers(host).await;
        state.write().await.docker_count = count;
    }
    // Stream kết thúc → caller giữ count cũ, retry sau RETRY_DELAY
}

async fn count_containers(host: &str) -> u32 {
    let out = docker_cmd(host)
        .args(["ps", "-q"])
        .output()
        .await
        .unwrap_or_else(|_| std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: vec![],
            stderr: vec![],
        });

    std::str::from_utf8(&out.stdout)
        .unwrap_or("")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count() as u32
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

fn docker_cmd(host: &str) -> tokio::process::Command {
    let mut c = tokio::process::Command::new("docker");
    c.env("PATH", crate::utils::full_path());
    c.env("DOCKER_HOST", host);
    c
}
