// Status-line visual renderer compiling state structures into unified Nerd Font layouts.

use crate::state::{PerPaneContext, SharedState};

// ── Nerd Font v3 Icons ──

const ICON_FOLDER: &str = "󰉋";
const ICON_BRANCH: &str = "󰘬";
const ICON_AI: &str = "󰚩";

const ICON_PG: &str = "";
const ICON_REDIS: &str = "";
const ICON_MYSQL: &str = "";
const ICON_MONGO: &str = "";

const ICON_DOCKER: &str = "";
const ICON_NODE: &str = "󰎙";
const ICON_VITE: &str = "󰉁";
const ICON_PYTHON: &str = "";
const ICON_JUPYTER: &str = "";
const ICON_HTTP: &str = "󱓞";

const ICON_ORBSTACK: &str = "󰙑";
const ICON_K8S: &str = "󱃾";
const ICON_BREW: &str = "󰏗";
const ICON_RAM: &str = "󰍛";

const SEP: &str = "  •  ";

// ── Helper Resolvers ──

fn brew_icon(name: &str) -> &'static str {
    let base = name.split('@').next().unwrap_or(name);

    match base {
        "nginx" | "caddy" => ICON_HTTP,
        "dnsmasq" => "󰖟",
        "minio" => "󰉋",
        "rabbitmq" => "󰳖",
        "kafka" => "󰿟",
        "vault" => "󰌆",
        "consul" => "󰟾",
        "etcd" => "󰈀",
        "prometheus" => "󱓽",
        "grafana" => "󱍃",
        _ => ICON_BREW,
    }
}

// ── Layout Construction Engine ──

pub async fn build(state: &SharedState, pane: &PerPaneContext) -> String {
    let s = state.read().await;
    let mut parts: Vec<String> = Vec::with_capacity(8);

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

    if let Some(model) = &s.ollama_model {
        let short: String = model.chars().take(12).collect();
        parts.push(format!("{ICON_AI} {short}"));
    }

    let mut db: Vec<&str> = Vec::new();
    if s.listening_ports.contains(&5432) { db.push(ICON_PG); }
    if s.listening_ports.contains(&6379) { db.push(ICON_REDIS); }
    if s.listening_ports.contains(&3306) { db.push(ICON_MYSQL); }
    if s.listening_ports.contains(&27017) { db.push(ICON_MONGO); }
    if !db.is_empty() {
        parts.push(db.join(" "));
    }

    if s.docker_count > 0 {
        parts.push(format!("{ICON_DOCKER} {}", s.docker_count));
    }

    if s.ram_percent >= 80.0 {
        parts.push(format!("{ICON_RAM} {:.0}% ", s.ram_percent));
    }

    let mut dev: Vec<&str> = Vec::new();
    for &port in &[3001u16, 5001, 5173, 8000, 8080, 8888] {
        if !s.listening_ports.contains(&port) { continue; }
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

    if s.orbstack_running {
        parts.push(format!("{ICON_ORBSTACK} orb"));
    }

    if let Some(ctx) = &s.kubernetes_context {
        parts.push(format!("{ICON_K8S} {ctx}"));
    }

    if !s.brew_services.is_empty() {
        let mut brew_icons: Vec<&str> = s.brew_services.iter().map(|n| brew_icon(n)).collect();
        brew_icons.sort_unstable();
        brew_icons.dedup();
        parts.push(brew_icons.join(" "));
    }

    let path_display = format!("{ICON_FOLDER}  {}", shorten_path(&pane.pane_path));

    if parts.is_empty() {
        format!("{path_display} ")
    } else {
        format!("{path_display}{SEP}{} ", parts.join(SEP))
    }
}

// ── Path String Shorters ──

fn shorten_path(path: &str) -> String {
    shorten_path_with_home(path, crate::utils::home_dir())
}

fn shorten_path_with_home(path: &str, home: &str) -> String {
    let display = if !home.is_empty() && path.starts_with(home) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    };

    let parts: Vec<&str> = display.split('/').collect();
    let n = parts.len();
    parts[n.saturating_sub(3)..].join("/")
}

// ── Unit Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_path_replaces_home_with_tilde() {
        assert_eq!(
            shorten_path_with_home("/Users/dev/projects/tmuxd", "/Users/dev"),
            "~/projects/tmuxd"
        );
    }

    #[test]
    fn shorten_path_keeps_only_last_three_components() {
        assert_eq!(
            shorten_path_with_home("/a/b/c/d/e/f", "/nonexistent"),
            "d/e/f"
        );
    }

    #[test]
    fn shorten_path_leaves_short_paths_untouched() {
        assert_eq!(shorten_path_with_home("/a/b", "/nonexistent"), "/a/b");
    }

    #[test]
    fn shorten_path_ignores_home_if_path_does_not_start_with_it() {
        assert_eq!(
            shorten_path_with_home("/opt/Users/dev/x", "/Users/dev"),
            "test/Users/dev/x".replace("test/", "")
        );
    }

    #[test]
    fn brew_icon_known_services_get_specific_icons() {
        assert_eq!(brew_icon("nginx"), ICON_HTTP);
        assert_eq!(brew_icon("kafka"), "󰿟");
    }

    #[test]
    fn brew_icon_versioned_service_name_matches_base() {
        assert_eq!(brew_icon("vault@1.15"), brew_icon("vault"));
    }

    #[test]
    fn brew_icon_unknown_service_falls_back_to_generic_icon() {
        assert_eq!(brew_icon("some-random-service"), ICON_BREW);
    }
}