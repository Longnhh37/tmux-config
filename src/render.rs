// render.rs
//
// Two inputs, clearly separated:
//
//   GlobalState    → docker_count, listening_ports, ollama_model,
//                    colima_running, kubernetes_context, brew_services
//                    same value for every pane at any given moment
//
//   PerPaneContext → pane_path, git
//                    changes when you switch to a pane in a different directory

use crate::state::{PerPaneContext, SharedState};

// ── Nerd Font v3 icons ──────────────────────────
const ICON_FOLDER: &str = "󰉋"; // nf-md-folder
const ICON_BRANCH: &str = "󰘬"; // nf-md-source_branch
const ICON_AI: &str = "󰚩"; // nf-md-robot (ollama)

// Databases
const ICON_PG: &str = ""; // nf-dev-postgresql
const ICON_REDIS: &str = ""; // nf-dev-redis
const ICON_MYSQL: &str = ""; // nf-dev-mysql
const ICON_MONGO: &str = ""; // nf-dev-mongodb

// Dev tools / runtime
const ICON_DOCKER: &str = ""; // nf-dev-docker
const ICON_NODE: &str = "󰎙"; // nf-md-nodejs
const ICON_VITE: &str = "󰉁"; // nf-seti-vite
const ICON_PYTHON: &str = ""; // nf-dev-python
const ICON_JUPYTER: &str = ""; // nf-dev-jupyter

// Network / generic
const ICON_HTTP: &str = "󱓞"; // generic port

// ── Manually-started services ─────────────
const ICON_ORBSTACK: &str = "󰙑"; // 󰙑  nf-md-orbit
const ICON_K8S: &str = "󱃾"; // nf-md-kubernetes
const ICON_BREW: &str = "󰏗"; // nf-md-package-variant

// ── RAM ─────────────
const ICON_RAM: &str = "󰍛"; // nf-md-memory

// ── others ─────────────
const SEP: &str = "  •  ";

// Brew service name → icon (các service không có port riêng)
fn brew_icon(name: &str) -> &'static str {
    let base = name.split('@').next().unwrap_or(name);

    match base {
        "nginx" => ICON_HTTP,
        "caddy" => ICON_HTTP,

        "dnsmasq" => "󰖟",  // nf-md-dns
        "minio" => "󰉋",    // nf-md-folder
        "rabbitmq" => "󰳖", // nf-md-rabbit
        "kafka" => "󰿟",    // nf-md-apache_kafka

        // useful additions
        "vault" => "󰌆",      // nf-md-vault
        "consul" => "󰟾",     // nf-md-server_network
        "etcd" => "󰈀",       // nf-md-database
        "prometheus" => "󱓽", // nf-md-chart_timeline_variant
        "grafana" => "󱍃",    // nf-md-chart_donut

        _ => ICON_BREW,
    }
}

/// Build the full right-status string for a pane.
///
/// Per-pane  (changes on pane/tab switch):  path, git branch/diff
/// Persistent (same across all panes):      ollama, ports, docker,
///                                          colima, k8s, brew services, orbstack
pub async fn build(state: &SharedState, pane: &PerPaneContext) -> String {
    let s = state.read().await;
    let mut parts: Vec<String> = Vec::with_capacity(8);

    // ── Git — per-pane ───────────────────────────────────────────────────────
    if let Some(g) = &pane.git {
        let mut entry = format!("{ICON_BRANCH} {}:{}", g.repo, g.branch);
        if g.changed > 0 {
            entry.push_str(&format!(" ~{}", g.changed));
        }
        if g.insertions > 0 {
            entry.push_str(&format!(" +{}", g.insertions));
        }
        if g.deletions > 0 {
            entry.push_str(&format!(" -{}", g.deletions));
        }
        parts.push(entry);
    }

    // ── Ollama — global / persistent ─────────────────────────────────────────
    if let Some(model) = &s.ollama_model {
        let short: String = model.chars().take(12).collect();
        parts.push(format!("{ICON_AI} {short}"));
    }

    // ── Databases — global / persistent ──────────────────────────────────────
    let mut db: Vec<&str> = Vec::new();
    if s.listening_ports.contains(&5432) {
        db.push(ICON_PG);
    }
    if s.listening_ports.contains(&6379) {
        db.push(ICON_REDIS);
    }
    if s.listening_ports.contains(&3306) {
        db.push(ICON_MYSQL);
    }
    if s.listening_ports.contains(&27017) {
        db.push(ICON_MONGO);
    }
    if !db.is_empty() {
        parts.push(db.join(" "));
    }

    // ── Docker — global / persistent ─────────────────────────────────────────
    if s.docker_count > 0 {
        parts.push(format!("{ICON_DOCKER} {}", s.docker_count));
    }

    // ── RAM pressure — global / persistent ───────────────────────────────────
    // Chỉ hiện khi RAM căng (>=80%), tránh làm rối status bar lúc bình thường.
    if s.ram_percent >= 80.0 {
        parts.push(format!("{ICON_RAM} {:.0}% ", s.ram_percent));
    }

    // ── Dev servers — global / persistent ────────────────────────────────────
    let mut dev: Vec<&str> = Vec::new();
    for &port in &[3001u16, 5001, 5173, 8000, 8080, 8888] {
        if !s.listening_ports.contains(&port) {
            continue;
        }
        dev.push(match port {
            3001 => ICON_NODE,
            5173 => ICON_VITE,
            5001 | 8000 => ICON_PYTHON,
            8080 => ICON_HTTP,
            8888 => ICON_JUPYTER,
            _ => ICON_HTTP,
        });
    }
    if !dev.is_empty() {
        parts.push(dev.join(" "));
    }

    // ── OrbStack — global / persistent ─────────────────────────────────────────
    if s.orbstack_running {
        parts.push(format!("{ICON_ORBSTACK} orb"));
    }

    // ── Kubernetes — global / persistent ─────────────────────────────────────
    // Hiện context name khi cluster đang reachable.
    // VD: "󱃾 colima" / "󱃾 gke/prod" / "󱃾 eks/staging"
    if let Some(ctx) = &s.kubernetes_context {
        parts.push(format!("{ICON_K8S} {ctx}"));
    }

    // ── Brew services — global / persistent ──────────────────────────────────
    // Chỉ những service không được hiện bởi port detection
    // (redis/postgres/mysql/mongo đã có icon database ở trên).
    if !s.brew_services.is_empty() {
        let mut brew_icons: Vec<&str> = s.brew_services.iter().map(|n| brew_icon(n)).collect();
        brew_icons.sort_unstable(); // thứ tự ổn định giữa các lần render
        brew_icons.dedup();
        parts.push(brew_icons.join(" "));
    }

    // ── Path — per-pane (always leftmost) ────────────────────────────────────
    let path_display = format!("{ICON_FOLDER}  {}", shorten_path(&pane.pane_path));

    if parts.is_empty() {
        format!("{path_display} ")
    } else {
        format!("{path_display}{SEP}{} ", parts.join(SEP))
    }
}

/// Replace $HOME with ~, then keep only the last 3 path components.
fn shorten_path(path: &str) -> String {
    let home = crate::utils::home_dir();

    let display = if !home.is_empty() && path.starts_with(home) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    };

    let parts: Vec<&str> = display.split('/').collect();
    let n = parts.len();
    parts[n.saturating_sub(3)..].join("/")
}
