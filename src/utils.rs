use std::path::PathBuf;
use std::sync::OnceLock;

pub fn home_dir() -> &'static str {
    static HOME: OnceLock<String> = OnceLock::new();
    HOME.get_or_init(|| std::env::var("HOME").unwrap_or_default())
}

pub fn full_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let home = home_dir();
        let existing = std::env::var("PATH").unwrap_or_default();
        let extra = format!("/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:{home}/.local/bin");
        if existing.is_empty() {
            extra
        } else {
            format!("{extra}:{existing}")
        }
    })
}

pub fn sock_path() -> PathBuf {
    std::env::temp_dir().join("tmuxd.sock")
}

pub fn cache_path(pane_id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("tmux_status_{pane_id}"))
}
