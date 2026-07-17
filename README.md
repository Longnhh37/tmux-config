# tmux config + tmuxd – tmux status bar collector 

`tmuxd` is a high-performance, asynchronous background daemon written in Rust, specifically optimized to serve status bar data for `tmux` on macOS (M-series). Instead of constantly spawning shell processes (shell-outs) which cause UI lag and high CPU usage, `tmuxd` runs quietly in the background, aggregates system context globally, and responds to tmux pane queries in `<5ms`.

## Architecture & Flow

The system is designed around a decoupled architecture separating heavy, slow global metrics from fast, path-specific contextual data.

```text
                              ┌─────────────────────────────────────────┐
                              │           tmuxd daemon                  │
                              │                                         │
┌────────────┐                │  ┌───────────────────────────────────┐  │
│            │                │  │ Background Collectors (Async)     │  │
│ tmux (UI)  │                │  │  ├─ Docker (Unix Socket HTTP)     │  │
│            │                │  │  ├─ Ollama (reqwest HTTP)         │  │
└─────┬──────┘                │  │  ├─ K8s, Brew, OrbStack Services  │  │
      │                       │  │  ├─ Active TCP Ports              │  │
      │ 1. query              │  │  └─ RAM Pressure                  │  │
      │    pane_id            │  └─────────────────┬─────────────────┘  │
      │    pane_path          │                    │ 3. continuously    │
      ▼                       │                    │    updates         │
┌────────────┐                │                    ▼                    │
│            │ 2. connects    │  ┌───────────────────────────────────┐  │
│ tmuxd      ├─ Unix Socket ─►│  │ Global State (In-Memory RwLock)   │  │
│ status     │  ($TMPDIR)     │  └─────────────────┬─────────────────┘  │
│            ◄─ 6. returns  ─ ┤                    │ 4. reads           │
└────────────┘    string      │                    ▼                    │
                              │  ┌───────────────────────────────────┐  │
                              │  │ Render Engine (Nerd Fonts)        │  │
                              │  └─────────────────▲─────────────────┘  │
                              │                    │ 5. reads           │
                              │  ┌─────────────────┴─────────────────┐  │
                              │  │ Per-Pane Context                  │  │
                              │  │  └─ Git (stale-while-revalidate)  │  │
                              │  └───────────────────────────────────┘  │
                              └─────────────────────────────────────────┘
```

### Core Workflows

The application is split into two independent lifecycles communicating via a Unix Socket:

1. **Background Collectors (`daemon`)**
   - Runs continuously in the background utilizing the `tokio` async runtime.
   - Responsible for tracking slow-to-fetch global metrics (Docker container counts, Kubernetes contexts, Brew services, Ollama models, open ports, and RAM pressure).
   - **Zero-Shell-Out Optimization:** Modules like Docker and Ollama communicate directly via native HTTP (over Unix sockets or TCP), completely eliminating the overhead of spawning external CLI dependencies.

2. **Client Query (`status`)**
   - Invoked by tmux on status bar redraw: `tmuxd status <pane_id> <pane_path>`.
   - Connects to the daemon via a secure, user-specific Unix socket in macOS (`$TMPDIR/tmuxd.sock`).
   - The daemon immediately resolves `pane_path`-specific data (like Git branch and diffs via `libgit2` using a non-blocking stale-while-revalidate cache), merges it with the latest snapshot of the Global State, and returns a fully formatted Nerd Font string.

## Key Optimizations for macOS

* **Native API Integrations:** Bypasses `docker` and `xh`/`curl` binaries by interacting directly with the Docker Engine API and Ollama API.
* **Race-Condition Safety:** Improved socket binding logic preventing `EADDRINUSE` errors on restart.
* **Graceful Shutdowns:** Listens to `SIGTERM` / `SIGINT` to cleanly remove socket files upon exit.
* **Secure Cache & Sockets:** Utilizes macOS `$TMPDIR` instead of the globally shared `/tmp` directory for enhanced security and collision avoidance.

## Directory Structure

* `client.rs` / `server.rs`: Handles the Unix Socket request/response protocol, signal handling, and fallback file caching.
* `collectors/`: Modules that poll system metrics at specific intervals (Docker, Ollama, K8s, Ports, RAM).
* `git.rs`: Per-pane Git repository state resolver using a TTL-based cache.
* `render.rs`: The formatting engine that concatenates states into a Nerd Font icon string.
* `state.rs`: Defines the shared `RwLock` data structures holding the global context.
