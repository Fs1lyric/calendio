#!/usr/bin/env bash
# Screenshots every calendar view on a synthetic database, inside a private Xvfb
# display that holds nothing but Calendio. Meant for CI; it never touches a real
# calendar or a real screen.
#
#   tests/screenshots.sh [out-dir]    (needs Xvfb and ImageMagick's `import`)
set -euo pipefail

out=${1:-screenshots}
mkdir -p "$out"
scratch=$(mktemp -d)
export CALENDIO_DB="$scratch/demo.db"
unset WAYLAND_DISPLAY
export LIBGL_ALWAYS_SOFTWARE=1

cargo build -q -p calendio
cargo run -q -p calendio-core --example demo_seed

display=:99
Xvfb "$display" -screen 0 1180x780x24 -nolisten tcp &
xvfb=$!
trap 'kill $xvfb 2>/dev/null; rm -rf "$scratch"' EXIT
export DISPLAY=$display
sleep 1

shoot() {
    local name=$1
    shift
    ./target/debug/calendio "$@" &
    local app=$!
    sleep 5
    import -window root "$out/$name.png"
    kill "$app"
    wait "$app" 2>/dev/null || true
}

for theme in white dark; do
    for view in week month day year agenda; do
        shoot "$view-$theme" --view "$view" --theme "$theme"
    done
    shoot "editor-$theme" --view week --theme "$theme" --new
done

ls "$out"
