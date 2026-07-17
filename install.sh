#!/usr/bin/env bash
set -euo pipefail

BIN="$HOME/.local/bin/tmuxd"

echo "-> Building tmuxd..."
cargo build --release

echo "installing to $BIN"
mkdir -p "$(dirname "$BIN")"
cp target/release/tmuxd "$BIN"

echo "-> killing old daemon (if any)"
pkill -x tmuxd 2>/dev/null || true
sleep 0.3

echo "starting daemon..."
"$BIN" daemon &>/tmp/tmuxd.log &

echo "reloading tmux config..."
tmux source-file ~/.tmux.conf 2>/dev/null || true

echo "Done"
