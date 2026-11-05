#!/usr/bin/env bash
# Installs Calendio for this user: the binaries in ~/.local/bin, the launcher
# entry and icon under ~/.local/share, and the booker as a systemd user service
# on 127.0.0.1:8788.
#
# By default it downloads the binaries GitHub Actions built from the latest green
# run on main, so nothing compiles on this machine. Pass --build to build here.
set -euo pipefail
cd "$(dirname "$0")"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
if [[ "${1:-}" == "--build" ]]; then
    export PATH="$HOME/.cargo/bin:$PATH"
    nice -n 19 cargo build --release -p calendio -p calendio-booker -j 2
    from=target/release
else
    run=$(gh run list --workflow CI --branch main --status success --limit 1 --json databaseId -q '.[0].databaseId')
    [[ -n "$run" ]] || { echo "no successful CI run on main yet" >&2; exit 1; }
    from=$(mktemp -d)
    trap 'rm -r "$from"' EXIT
    gh run download "$run" --name calendio-linux-x86_64 --dir "$from"
    echo "binaries from CI run $run"
fi
install -Dm755 "$from/calendio" "$HOME/.local/bin/calendio"
install -Dm755 "$from/calendio-booker" "$HOME/.local/bin/calendio-booker"
install -Dm644 packaging/calendio-booker.service "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/calendio-booker.service"
install -Dm644 packaging/calendio.desktop "$data/applications/calendio.desktop"
install -Dm644 packaging/calendio.svg "$data/icons/hicolor/scalable/apps/calendio.svg"
update-desktop-database -q "$data/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$data/icons/hicolor" 2>/dev/null || true
systemctl --user daemon-reload
systemctl --user enable calendio-booker.service
systemctl --user restart calendio-booker.service
echo "installed: $HOME/.local/bin/calendio, booking page on http://127.0.0.1:8788/<link>"
