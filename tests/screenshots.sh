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
# Dummy backup files so the restore list has rows; the list only reads their names.
export CALENDIO_BACKUP_DIR="$scratch/backups"
mkdir -p "$CALENDIO_BACKUP_DIR"
for days in 0 1 2 5; do
    touch "$CALENDIO_BACKUP_DIR/calendio-$(date -u -d "-$days days" +%F).sqlite3"
done
touch "$CALENDIO_BACKUP_DIR/calendio-$(date -u +%F)-$(date -u -d '-1 hour' +%H%M%S).sqlite3"
unset WAYLAND_DISPLAY
export LIBGL_ALWAYS_SOFTWARE=1
# A US clock, so the shots show 12-hour times; Calendio reads it from the locale.
export LC_TIME=en_US.UTF-8

cargo build -q -p calendio
cargo run -q -p calendio-core --example demo_seed

display=:99
Xvfb "$display" -screen 0 1600x780x24 -nolisten tcp &
xvfb=$!
trap 'kill $xvfb 2>/dev/null; rm -rf "$scratch"' EXIT
export DISPLAY=$display
sleep 1

# shoot NAME WIDTH ARGS...: a WIDTHx780 window in the top-left corner, cropped to it.
shoot() {
    local name=$1 width=$2
    shift 2
    ./target/debug/calendio --size "${width}x780" "$@" &
    local app=$!
    # The first launch can take longer than 5 s to draw; retake a black frame.
    for wait in 5 4 4; do
        sleep "$wait"
        import -window root -crop "${width}x780+0+0" +repage "$out/$name.png"
        [ "$(convert "$out/$name.png" -format '%[fx:mean>0]' info:)" = 1 ] && break
    done
    kill "$app"
    wait "$app" 2>/dev/null || true
}

for theme in light dark; do
    for view in week month day year agenda; do
        shoot "$view-$theme" 1180 --view "$view" --theme "apple-$theme"
    done
    shoot "editor-$theme" 1180 --view week --theme "apple-$theme" --new
    for view in week month day; do
        shoot "$view-$theme-390" 390 --view "$view" --theme "apple-$theme"
    done
done
shoot recovery-light 1180 --view week --theme apple-light --recovery
# Wide enough for the sidebar's today panel.
shoot week-light-1500 1500 --view week --theme apple-light

ls "$out"
