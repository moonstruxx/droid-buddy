#!/usr/bin/env bash
# Live visual proof harness for the overlay-dispatch change.
#
# The session is GNOME Wayland with no grim/wtype/ydotool, and Xwayland key
# injection does not reach a mutter-managed window, so keys cannot be driven.
# Instead each surface is forced open at startup through a TEMPORARY env hook
# (DROID_TUI_DEBUG_OVERLAY) and the real window is screenshotted via `import`.
# The hook is removed after verification.
#
# Usage: shot_overlay.sh <overlay>   where overlay is one of:
#   picker | validation | select | optimizer | diff | help | label
set -u

OVERLAY="${1:?usage: shot_overlay.sh <picker|validation|select|optimizer|diff|help|label>}"
ROOT=/home/bjoern/projects/droid_tui
OUT="$ROOT/.opencode/.tmp/keytest"
PATCH="${2:-$ROOT/fixtures/arpeggio1.ini}"

mkdir -p "$OUT"
SHOT="$OUT/live_${OVERLAY}.png"
LOG="/tmp/dt_${OVERLAY}_stdout.log"

env -u WAYLAND_DISPLAY \
    DROID_TUI_DEBUG_OVERLAY="$OVERLAY" \
    ./target/debug/droid_tui "$PATCH" >"$LOG" 2>&1 &
APP=$!

WID=""
for _ in $(seq 1 48); do
    kill -0 "$APP" 2>/dev/null || { echo "APP DIED ($OVERLAY)"; tail -5 "$LOG"; exit 1; }
    WID=$(xdotool search --name "droid_tui" 2>/dev/null | tail -1)
    [ -n "$WID" ] && break
    sleep 0.25
done
[ -n "$WID" ] || { echo "no window ($OVERLAY)"; kill "$APP" 2>/dev/null; exit 1; }

sleep 2.0
import -window "$WID" "$SHOT" 2>/dev/null && echo "captured $SHOT"
kill "$APP" 2>/dev/null
wait "$APP" 2>/dev/null
pkill -x droid_tui 2>/dev/null

magick "$SHOT" -resize 1400x -quality 88 "${SHOT%.png}.jpg" 2>/dev/null
python3 - "$SHOT" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('RGB')
px = im.load(); w, h = im.size
nonblack = sum(1 for y in range(0, h, 4) for x in range(0, w, 4) if sum(px[x, y]) > 30)
print(f"{sys.argv[1]}: {w}x{h} nonblack_sampled={nonblack} colors={len(im.getcolors(maxcolors=200000) or [])}")
PY
