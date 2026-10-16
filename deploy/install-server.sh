#!/usr/bin/env bash
# Installs the Susic server on Ubuntu/Debian with systemd.
# Non-destructive: never touches disks, mounts or existing config, and never
# modifies your music directory. Re-running upgrades the binary only.
#
#   sudo deploy/install-server.sh [path/to/susic-server]
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "Run as root (sudo)." >&2; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN="${1:-$HERE/../target/release/susic-server}"
[ -x "$BIN" ] || { echo "Binary not found: $BIN (build with: cargo build --release -p susic-server)" >&2; exit 1; }

id susic >/dev/null 2>&1 || useradd --system --home-dir /var/lib/susic --shell /usr/sbin/nologin susic
install -d -o susic -g susic -m 0750 /var/lib/susic
install -d -m 0755 /etc/susic
install -m 0755 "$BIN" /usr/local/bin/susic-server
if [ -e /etc/susic/server.toml ]; then
  echo "Keeping existing /etc/susic/server.toml"
else
  install -m 0644 "$HERE/server.example.toml" /etc/susic/server.toml
  echo "Wrote /etc/susic/server.toml — edit music_dir and bind before starting."
fi
install -m 0644 "$HERE/susic-server.service" /etc/systemd/system/susic-server.service
systemctl daemon-reload
cat <<MSG

Installed. Next:
  1. Edit /etc/susic/server.toml (music_dir, bind).
  2. sudo -u susic susic-server --config /etc/susic/server.toml check
  3. sudo systemctl enable --now susic-server
  4. sudo -u susic susic-server --config /etc/susic/server.toml pair
  5. curl -s http://<bind-address>/health
Optional YouTube import (ytdlp_enabled = true) needs a writable imports folder
and a systemd drop-in: see docs/SERVER.md, "Enabling YouTube import under systemd".
MSG
