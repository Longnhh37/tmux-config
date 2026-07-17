// collectors/ollama.rs

use crate::state::SharedState;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::process::Command;

const POLL_INTERVAL: Duration = Duration::from_secs(3);
const API_URL: &str = "http://localhost:11434/api/ps";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

const NOT_RUNNING_THRESHOLD: u32 = 2;

pub async fn run(state: SharedState) {
    let mut consecutive_not_running: u32 = 0;

    loop {
        if is_ollama_running().await {
            consecutive_not_running = 0;

            // Lấy tên model đang chạy. Nếu API trả về None (chưa load model nào),
            // mặc định hiển thị chữ "ollama" để luôn hiện icon 󰚩.
            let active_model = fetch_active_model().await;
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
    // Dùng `ps` thay cho `pgrep` để tối ưu tốc độ và không phụ thuộc.
    // Lấy danh sách tên tất cả process đang chạy.
    let output = Command::new("ps")
        .env("PATH", crate::utils::full_path())
        .args(["-e", "-o", "comm="])
        .output()
        .await;

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        // Duyệt qua kết quả trong RAM thay vì dùng shell pipeline
        stdout.lines().any(|line| line.trim() == "ollama")
    } else {
        false
    }
}

/// Shared HTTP client, built once and reused across polls (keeps the
/// connection to localhost:11434 warm instead of reconnecting every 3s).
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default()
    })
}

async fn fetch_active_model() -> Option<String> {
    // In-process HTTP call instead of shelling out to `xh`: no external
    // binary dependency, and skips the ~5-10ms cost of spawning a process
    // on every poll.
    let json: serde_json::Value = http_client()
        .get(API_URL)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

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
