// Background collector measuring macOS system memory pressure levels.

use crate::state::SharedState;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::process::Command;

const POLL_INTERVAL: Duration = Duration::from_secs(3);

// ── Main Loop ──

pub async fn run(state: SharedState) {
    loop {
        if let Some(percent) = read_ram_percent().await {
            state.write().await.ram_percent = percent;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ── Sysctl & VmStat Interface ──

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