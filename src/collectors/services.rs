// collectors/services.rs
//
// Detects manually-started services that ports.rs không cover được:
//
//   Kubernetes  → kubectl current-context + TCP probe API server
//   Brew        → `brew services list --json`  (bỏ qua các service đã hiện qua port)
//
// Poll interval: 15 s (chậm hơn ports.rs vì các lệnh này tốn kém hơn)

use std::collections::HashSet;
use std::time::Duration;

use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::process::Command;

use crate::state::SharedState;

const POLL_INTERVAL: Duration = Duration::from_secs(15);

const PORT_COVERED: &[&str] = &[
    "redis",
    "postgresql",
    "postgresql@14",
    "postgresql@16",
    "mysql",
    "mysql@8.0",
    "mongodb-community",
];

pub async fn run(state: SharedState) {
    loop {
        let (orbstack, k8s, brew) =
            tokio::join!(check_orbstack(), check_kubernetes(), check_brew_services(),);

        {
            let mut s = state.write().await;
            s.orbstack_running = orbstack;
            s.kubernetes_context = k8s;
            s.brew_services = brew;
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ── OrbStack ────────────────────────────────────────────────────────────────
//
// Ưu tiên:
//   1. docker.sock của OrbStack
//   2. orbctl status
//

async fn check_orbstack() -> bool {
    let home = crate::utils::home_dir();

    let candidates = [
        format!("{home}/.orbstack/run/docker.sock"),
        format!("{home}/Library/Containers/com.orbstack.mac/Data/docker.sock"),
    ];
    for path in &candidates {
        if tokio::fs::metadata(path).await.is_ok() {
            return true;
        }
    }

    let tcp = tokio::time::timeout(
        Duration::from_millis(300),
        TcpStream::connect("127.0.0.1:3847"),
    )
    .await;
    if matches!(tcp, Ok(Ok(_))) {
        return true;
    }

    // Last resort: orbctl
    tokio::time::timeout(
        Duration::from_millis(500),
        Command::new("orbctl")
            .args(["status"])
            .env("PATH", crate::utils::full_path())
            .output(),
    )
    .await
    .ok()
    .and_then(|r| r.ok())
    .map(|o| o.status.success())
    .unwrap_or(false)
}

// ── Kubernetes ───────────────────────────────────────────────────────────────
//
// Bước 1: kubectl config current-context  → tên context (đọc file local, rất nhanh)
// Bước 2: TCP probe API server             → xác nhận cluster đang thực sự chạy
//
// Không dùng `kubectl cluster-info` vì nó làm full TLS handshake có thể
// mất vài giây nếu cluster không phản hồi.

#[derive(Deserialize)]
struct KubeConfig {
    #[serde(rename = "current-context")]
    current_context: String,
    clusters: Vec<KubeCluster>,
    contexts: Vec<KubeContextEntry>,
}
#[derive(Deserialize)]
struct KubeContextEntry {
    name: String,
    context: KubeContext,
}
#[derive(Deserialize)]
struct KubeContext {
    cluster: String,
}
#[derive(Deserialize)]
struct KubeCluster {
    name: String,
    cluster: ClusterDetails,
}
#[derive(Deserialize)]
struct ClusterDetails {
    server: String,
}

async fn check_kubernetes() -> Option<String> {
    let home = crate::utils::home_dir();
    let kube_path = format!("{home}/.kube/config");

    let yaml = tokio::fs::read_to_string(&kube_path).await.ok()?;
    let config: KubeConfig = serde_yaml::from_str(&yaml).ok()?;

    if config.current_context.is_empty() || config.current_context == "N/A" {
        return None;
    }

    let active_ctx = config
        .contexts
        .iter()
        .find(|c| c.name == config.current_context)?;
    let cluster_name = &active_ctx.context.cluster;
    let active_cluster = config.clusters.iter().find(|c| c.name == *cluster_name)?;
    let server_url = &active_cluster.cluster.server;

    let addr = server_url
        .trim_start_matches("https://")
        .trim_start_matches("http://");

    let reachable =
        tokio::time::timeout(Duration::from_millis(400), TcpStream::connect(addr)).await;

    match reachable {
        Ok(Ok(_)) => Some(shorten_context(&config.current_context)),
        _ => None,
    }
}

fn shorten_context(ctx: &str) -> String {
    if ctx.starts_with("gke_") {
        if let Some(name) = ctx.split('_').next_back() {
            return format!("gke/{name}");
        }
    }
    if ctx.contains(":cluster/") {
        if let Some(name) = ctx.split('/').next_back() {
            return format!("eks/{name}");
        }
    }
    ctx.chars().take(14).collect()
}

// ── Brew services ────────────────────────────────────────────────────────────
#[derive(Deserialize)]
struct BrewServiceEntry {
    name: String,
    running: bool,
}

async fn check_brew_services() -> HashSet<String> {
    let out = tokio::time::timeout(
        Duration::from_millis(800),
        Command::new("brew")
            .args(["services", "list", "--json"])
            .env("PATH", crate::utils::full_path())
            .output(),
    )
    .await;

    let output = match out {
        Ok(Ok(o)) if o.status.success() => o.stdout,
        _ => return HashSet::new(),
    };

    let text = std::str::from_utf8(&output).unwrap_or("");
    parse_brew_services_json(text)
}

fn parse_brew_services_json(json: &str) -> HashSet<String> {
    let port_covered: HashSet<&str> = PORT_COVERED.iter().copied().collect();

    let entries: Vec<BrewServiceEntry> = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return HashSet::new(),
    };

    entries
        .into_iter()
        .filter(|e| e.running)
        .filter(|e| {
            let base = e.name.split('@').next().unwrap_or(&e.name);
            !port_covered.contains(e.name.as_str()) && !port_covered.contains(base)
        })
        .map(|e| e.name)
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_brew_skips_port_covered() {
        let json = r#"[
            {"name":"redis","running":true,"loaded":true},
            {"name":"nginx","running":true,"loaded":true},
            {"name":"postgresql@14","running":false,"loaded":false}
        ]"#;
        let services = parse_brew_services_json(json);
        assert!(
            !services.contains("redis"),
            "redis should be skipped (port covered)"
        );
        assert!(services.contains("nginx"), "nginx should be included");
        assert!(
            !services.contains("postgresql@14"),
            "postgresql@14 not running"
        );
    }

    #[test]
    fn test_shorten_context_gke() {
        assert_eq!(
            shorten_context("gke_my-project_us-central1_prod"),
            "gke/prod"
        );
    }

    #[test]
    fn test_shorten_context_eks() {
        assert_eq!(
            shorten_context("arn:aws:eks:ap-northeast-1:123:cluster/staging"),
            "eks/staging"
        );
    }

    #[test]
    fn test_shorten_context_colima() {
        assert_eq!(shorten_context("colima"), "colima");
    }
}
