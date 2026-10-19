#!/usr/bin/env bash
# Builds Calendio and installs it for this user: the binaries in ~/.local/bin,
# the launcher entry and icon under ~/.local/share, and the booker as a
# systemd user service on 127.0.0.1:8788.
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release -p calendio -p calendio-booker
data="${XDG_DATA_HOME:-$HOME/.local/share}"
install -Dm755 target/release/calendio "$HOME/.local/bin/calendio"
install -Dm755 target/release/calendio-booker "$HOME/.local/bin/calendio-booker"
install -Dm644 packaging/calendio-booker.service "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/calendio-booker.service"
install -Dm644 packaging/calendio.desktop "$data/applications/calendio.desktop"
install -Dm644 packaging/calendio.svg "$data/icons/hicolor/scalable/apps/calendio.svg"
update-desktop-database -q "$data/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$data/icons/hicolor" 2>/dev/null || true
systemctl --user daemon-reload
systemctl --user enable calendio-booker.service
systemctl --user restart calendio-booker.service
echo "installed: $HOME/.local/bin/calendio, booking page on http://127.0.0.1:8788/<link>"
