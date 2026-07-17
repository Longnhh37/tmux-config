use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Base directory for the daemon socket and per-pane cache files.
///
/// Uses `std::env::temp_dir()` instead of a hardcoded `/tmp`: on macOS this
/// resolves to the per-user `$TMPDIR` (e.g. `/var/folders/xx/.../T/`), which
/// isn't a world-writable shared directory the way `/tmp` is.
pub fn runtime_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(std::env::temp_dir)
}

pub fn sock_path() -> PathBuf {
    runtime_dir().join("tmuxd.sock")
}

pub fn cache_path(pane_id: &str) -> PathBuf {
    runtime_dir().join(format!("tmux_status_{pane_id}"))
}

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
