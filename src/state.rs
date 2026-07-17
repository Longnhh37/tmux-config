use std::{collections::HashSet, sync::Arc};
use tokio::sync::RwLock;

/// Global state — shared across ALL panes, tabs, and windows.
/// Updated only by background collectors; never touched per-request.
///
///   docker_count      → collectors/docker.rs   (event-driven)
///   listening_ports   → collectors/ports.rs    (poll 3 s)
///   ollama_model      → collectors/ollama.rs   (poll 3 s)
///   colima_running    → collectors/services.rs (poll 5 s)
///   kubernetes_context→ collectors/services.rs (poll 5 s)
///   brew_services     → collectors/services.rs (poll 5 s)
#[derive(Default, Debug)]
pub struct GlobalState {
    /// Number of running Docker containers (0 = docker not running / no containers).
    pub docker_count: u32,

    /// TCP ports currently in LISTEN state.
    pub listening_ports: HashSet<u16>,

    /// The first loaded Ollama model name, truncated to 12 chars (None = ollama idle/absent).
    pub ollama_model: Option<String>,

    /// Current kubectl context name if the cluster is reachable, None otherwise.
    /// Shortened: "gke_proj_region_name" → "gke/name", "colima" → "colima".
    pub kubernetes_context: Option<String>,

    /// Brew services that are running AND not already shown via port detection.
    /// Examples: "nginx", "dnsmasq", "minio" — never "redis", "postgresql", etc.
    pub brew_services: HashSet<String>,

    /// OrbStack VM/runtime
    pub orbstack_running: bool,

    /// % RAM usage (0.0 if not ready)
    pub ram_percent: f32,

}

/// Per-pane context — derived fresh on every request from the pane's own
/// `pane_id` and `pane_current_path`.  Never stored in shared state.
///
///   path  → #{pane_current_path} from tmux
///   git   → git::get_cached(path)   (stale-while-revalidate, TTL 3 s)
pub struct PerPaneContext {
    pub pane_path: String,
    pub git: Option<crate::git::GitInfo>,
}

pub type SharedState = Arc<RwLock<GlobalState>>;
