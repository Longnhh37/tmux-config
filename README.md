# tmux config + tmuxd -- tmux status bar collector

## What it is

`tmuxd` is a daemon that renders a rich tmux status bar.

Instead of executing expensive commands (`git status`, `docker`,
`kubectl`, `brew`, ...) every time tmux redraws the status line, a
background daemon continuously collects machine state and keeps it in
memory.

When tmux needs to redraw the status bar, it only performs a lightweight
Unix socket request that typically completes in a few milliseconds.

------------------------------------------------------------------------

# Why?

A typical tmux status bar might execute commands such as

-   `git status`
-   `docker ps`
-   `kubectl config current-context`
-   `brew services list`

every second.

That means:

-   every pane repeats the same work
-   slow CLIs are spawned over and over
-   large Git repositories become noticeably laggy

`tmuxd` moves all expensive work into one background daemon.

Machine-wide information (Docker, RAM, Kubernetes, services, Ollama...)
is collected once and shared by every pane.

Only Git remains pane-specific.

------------------------------------------------------------------------

# Request Flow

``` text
┌────────────┐
│ tmux (UI)  │
└─────┬──────┘
      │
      │ status-right
      │ tmuxd status <pane_id> <pane_path>
      ▼
┌────────────────────┐
│ client.rs          │
│ (short-lived)      │
└─────────┬──────────┘
          │
          │ Unix socket
          ▼
┌────────────────────┐
│ server.rs          │
└─────────┬──────────┘
          │
          ├── read GlobalState
          ├── git.rs (per-path cache)
          ├── render.rs
          ▼
   rendered status string
          │
          ▼
       tmux status bar



If the daemon is unavailable:

client.rs
      │
      ▼
last-known cache
```

# Daemon Architecture

``` text
                        ┌──────────────────────────────┐
                        │         tmuxd daemon         │
                        └──────────────┬───────────────┘
                                       │
                 ┌─────────────────────┴─────────────────────┐
                 │                                           │
                 ▼                                           ▼
        Background collectors                    Unix socket server
          (tokio tasks)                             (server.rs)
                  |
                  ▼
 docker ──────────┐
 ollama ──────────┤
 ports────────────┤
 ram ─────────────┤
 services ────────┘
             │
             ▼
     Arc<RwLock<GlobalState>>
             │
             ▼
       request handler
             │
             ├── git.rs
             ├── render.rs
             ▼
       rendered status
```

# Collectors

      Collector              Responsibility
      -------------          -------------------------------------------------
      docker.rs              running containers + docker events
      ports.rs               listening development ports
      ollama.rs              loaded Ollama model
      services.rs            Kubernetes context, Homebrew services, OrbStack
      ram.rs                 RAM usage

# Requirements

-   macOS
-   Rust 2021
-   tmux ≥ 2.1
-   Nerd Font
