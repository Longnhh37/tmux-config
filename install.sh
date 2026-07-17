#!/usr/bin/env bash
set -euo pipefail

SRC_DIR="${1:-.}"
DEST_DIR="$HOME"
f="tmux.conf"

if [[ -f "$SRC_DIR/$f" ]]; then
  cp "$SRC_DIR/$f" "$DEST_DIR/.$f"
  echo "✔ Đã copy $f -> $DEST_DIR/.$f"
else
  echo "⚠ Không tìm thấy $SRC_DIR/$f"
  exit 1
fi

# Refresh tmux nếu đang có session chạy
if command -v tmux &>/dev/null && tmux info &>/dev/null; then
  tmux source-file "$DEST_DIR/.$f"
  echo "✅ Đã refresh tmux config"
else
  echo "ℹ tmux không chạy (hoặc không cài), config sẽ được áp dụng khi tmux khởi động lần sau"
fi

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
