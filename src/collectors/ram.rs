// RAM pressure (macOS) — `sysctl hw.memsize` (tổng RAM, cache 1 lần)
// + `vm_stat` (số liệu page hiện tại) để tính % đang dùng.
//
// Công thức xấp xỉ giống Activity Monitor:
//   used% = (active + wired + compressed) * page_size / total_ram
//
// Ngưỡng hiển thị được xử lý ở render.rs:
//   >= 80%  → icon RAM + warning
//   >= 95%  → icon RAM + alarming

use crate::state::SharedState;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::process::Command;

const POLL_INTERVAL: Duration = Duration::from_secs(3);

pub async fn run(state: SharedState) {
    loop {
        if let Some(percent) = read_ram_percent().await {
            state.write().await.ram_percent = percent;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Tổng RAM vật lý (bytes), chỉ đọc 1 lần trong suốt vòng đời daemon.
fn total_memory_bytes() -> Option<u64> {
    static TOTAL: OnceLock<Option<u64>> = OnceLock::new();
    *TOTAL.get_or_init(|| {
        let out = std::process::Command::new("sysctl")
            .env("PATH", crate::utils::full_path())
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        std::str::from_utf8(&out.stdout).ok()?.trim().parse().ok()
    })
}

async fn read_ram_percent() -> Option<f32> {
    let total = total_memory_bytes()? as f64;

    let out = Command::new("vm_stat")
        .env("PATH", crate::utils::full_path())
        .output()
        .await
        .ok()?;
    let text = std::str::from_utf8(&out.stdout).ok()?;

    // Dòng đầu: "Mach Virtual Memory Statistics: (page size of 16384 bytes)"
    let page_size = text
        .lines()
        .next()?
        .split("page size of ")
        .nth(1)?
        .split(' ')
        .next()?
        .parse::<f64>()
        .ok()?;

    let field = |name: &str| -> f64 {
        text.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split(':').nth(1))
            .map(|v| v.trim().trim_end_matches('.'))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };

    let active = field("Pages active");
    let wired = field("Pages wired down");
    let compressed = field("Pages occupied by compressor");

    let used_bytes = (active + wired + compressed) * page_size;
    Some(((used_bytes / total) * 100.0) as f32)
}
