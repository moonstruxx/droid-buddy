#!/usr/bin/env bash
set -u
LOG=/tmp/droid_tui_events.log
rm -f "$LOG"
# Force the X11 backend by hiding the Wayland display, so xdotool can inject keys.
env -u WAYLAND_DISPLAY DROID_DEBUG_EVENTS=1 ./target/debug/droid_tui >/tmp/droid_tui_stdout.log 2>&1 &
APP_PID=$!
echo "pid=$APP_PID"
WID=""
for i in $(seq 1 40); do
  kill -0 "$APP_PID" 2>/dev/null || { echo "DIED early"; cat /tmp/droid_tui_stdout.log; exit 1; }
  WID=$(xdotool search --name "droid_tui" 2>/dev/null | head -1)
  [ -n "$WID" ] && break
  sleep 0.25
done
echo "wid=$WID"
sleep 1
xdotool windowactivate --sync "$WID"; sleep 0.4
echo "active_wid=$(xdotool getactivewindow 2>/dev/null)"
echo "--- q ---"; xdotool key --clearmodifiers q; sleep 1.5
kill -0 "$APP_PID" 2>/dev/null && { echo "AFTER q: STILL ALIVE"; } || echo "AFTER q: EXITED"
echo "--- l (picker) ---"
kill -0 "$APP_PID" 2>/dev/null && { xdotool key --clearmodifiers l; sleep 1; echo "sent l"; }
kill -0 "$APP_PID" 2>/dev/null && kill "$APP_PID" 2>/dev/null
sleep 0.3
echo "=== EVENT LOG ==="
cat "$LOG" 2>/dev/null || echo "(no log)"
