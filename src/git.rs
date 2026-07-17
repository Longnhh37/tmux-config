// git.rs — per-pane, stale-while-revalidate cache keyed by path.
use git2::{Repository, StatusOptions};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const CACHE_TTL: Duration = Duration::from_secs(3);
const COLD_START_TIMEOUT: Duration = Duration::from_millis(150);

/// Paths không được access trong thời gian này sẽ bị evict khỏi cache.
/// Tránh HashMap lớn dần khi daemon chạy nhiều ngày với nhiều paths.
const EVICT_AFTER: Duration = Duration::from_secs(300); // 5 phút

#[derive(Clone)]
pub struct GitInfo {
    pub repo: String,
    pub branch: String,
    pub changed: u32,
    pub insertions: u32,
    pub deletions: u32,
}

struct CacheEntry {
    info: Option<GitInfo>,
    fetched: Instant,
    last_accessed: Instant,
}

static CACHE: std::sync::OnceLock<Arc<Mutex<HashMap<String, CacheEntry>>>> =
    std::sync::OnceLock::new();

fn cache() -> Arc<Mutex<HashMap<String, CacheEntry>>> {
    CACHE
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

pub async fn get_cached(path: &str) -> Option<GitInfo> {
    let cache = cache();

    let (cached_val, needs_refresh) = {
        let mut guard = cache.lock().await;

        // Evict paths không còn dùng (amortized — chạy mỗi lần acquire lock)
        guard.retain(|_, e| e.last_accessed.elapsed() < EVICT_AFTER);

        match guard.get_mut(path) {
            Some(entry) => {
                entry.last_accessed = Instant::now();
                let stale = entry.fetched.elapsed() > CACHE_TTL;
                (Some(entry.info.clone()), stale)
            }
            None => (None, true),
        }
    };

    match cached_val {
        Some(val) => {
            if needs_refresh {
                let path_owned = path.to_string();
                let cache2 = cache.clone();
                tokio::spawn(async move {
                    let fresh = fetch_git_info(&path_owned).await;
                    let mut guard = cache2.lock().await;
                    if let Some(entry) = guard.get_mut(&path_owned) {
                        // Chỉ update nếu entry vẫn còn (chưa bị evict)
                        entry.info = fresh;
                        entry.fetched = Instant::now();
                    }
                });
            }
            val
        }
        None => {
            // Cold start với timeout để không block tmux render
            let info = tokio::time::timeout(COLD_START_TIMEOUT, fetch_git_info(path))
                .await
                .unwrap_or(None);

            let result = info.clone();
            cache.lock().await.insert(
                path.to_string(),
                CacheEntry {
                    info,
                    fetched: Instant::now(),
                    last_accessed: Instant::now(),
                },
            );
            result
        }
    }
}

async fn fetch_git_info(path: &str) -> Option<GitInfo> {
    let path_owned = path.to_string();

    // Đưa git2 (đồng bộ) vào threadpool riêng để không block async runtime
    tokio::task::spawn_blocking(move || {
        let repo = Repository::discover(&path_owned).ok()?;
        let head = repo.head().ok()?;
        let branch = head.shorthand()?.to_string();

        let workdir = repo.workdir()?;
        let repo_name = workdir.file_name()?.to_string_lossy().into_owned();

        // Tính toán status (changed files)
        let mut opts = StatusOptions::new();
        opts.include_untracked(false);
        let statuses = repo.statuses(Some(&mut opts)).ok()?;

        let mut changed = 0;
        for entry in statuses.iter() {
            let status = entry.status();
            if status.is_wt_modified()
                || status.is_index_modified()
                || status.is_wt_deleted()
                || status.is_index_deleted()
            {
                changed += 1;
            }
        }

        // Tính Insertions/Deletions chính xác tương tự như diff --shortstat
        let mut insertions = 0;
        let mut deletions = 0;
        if let Ok(tree) = head.peel_to_tree() {
            if let Ok(diff) = repo.diff_tree_to_workdir_with_index(Some(&tree), None) {
                if let Ok(stats) = diff.stats() {
                    insertions = stats.insertions() as u32;
                    deletions = stats.deletions() as u32;
                }
            }
        }

        Some(GitInfo {
            repo: repo_name,
            branch,
            changed,
            insertions,
            deletions,
        })
    })
    .await
    .unwrap_or(None)
}
