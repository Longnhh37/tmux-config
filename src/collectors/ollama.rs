use crate::state::SharedState;
use std::time::Duration;
use tokio::process::Command;

const POLL_INTERVAL: Duration = Duration::from_secs(3);
const API_URL: &str = "http://localhost:11434/api/ps";
const NOT_RUNNING_THRESHOLD: u32 = 2;

pub async fn run(state: SharedState) {
    let mut consecutive_not_running: u32 = 0;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap_or_default();

    loop {
        if is_ollama_running().await {
            consecutive_not_running = 0;

            let active_model = fetch_active_model(&client).await;
            let display_text = active_model.unwrap_or_else(|| "ollama".to_string());

            state.write().await.ollama_model = Some(display_text);
        } else {
            consecutive_not_running += 1;

            if consecutive_not_running >= NOT_RUNNING_THRESHOLD {
                let mut s = state.write().await;
                if s.ollama_model.is_some() {
                    s.ollama_model = None;
                }
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn is_ollama_running() -> bool {
    let output = Command::new("ps")
        .env("PATH", crate::utils::full_path())
        .args(["-e", "-o", "comm="])
        .output()
        .await;

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        stdout.lines().any(|line| line.trim() == "ollama")
    } else {
        false
    }
}

async fn fetch_active_model(client: &reqwest::Client) -> Option<String> {
    let resp = client.get(API_URL).send().await.ok()?;
    let json: serde_json::Value = resp.json().await.ok()?;

    let name = json["models"]
        .as_array()?
        .first()?
        .get("name")?
        .as_str()?
        .to_string();

    let base: String = name
        .split(':')
        .next()
        .unwrap_or(&name)
        .chars()
        .take(12)
        .collect();

    Some(base)
}
