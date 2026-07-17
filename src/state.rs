// Definition of multi-thread safe data holders tracking machine parameters.

use std::{collections::HashSet, sync::Arc};
use tokio::sync::RwLock;

// ── Core Struct Specifications ──

#[derive(Default, Debug)]
pub struct GlobalState {
    pub docker_count: u32,
    pub listening_ports: HashSet<u16>,
    pub ollama_model: Option<String>,
    pub kubernetes_context: Option<String>,
    pub brew_services: HashSet<String>,
    pub orbstack_running: bool,
    pub ram_percent: f32,
}

pub struct PerPaneContext {
    pub pane_path: String,
    pub git: Option<crate::git::GitInfo>,
}

pub type SharedState = Arc<RwLock<GlobalState>>;