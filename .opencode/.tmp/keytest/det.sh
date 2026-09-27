#!/usr/bin/env bash
set -u
LOG=/tmp/droid_tui_events.log
rm -f "$LOG"
env -u WAYLAND_DISPLAY DROID_DEBUG_EVENTS=1 ./target/debug/droid_tui >/tmp/droid_tui_stdout.log 2>&1 &
APP_PID=$!
WID=""
for i in $(seq 1 40); do
  kill -0 "$APP_PID" 2>/dev/null || { echo "DIED"; cat /tmp/droid_tui_stdout.log; exit 1; }
  WID=$(xdotool search --name "droid_tui" 2>/dev/null | head -1)
  [ -n "$WID" ] && break
  sleep 0.25
done
echo "app_pid=$APP_PID wid=$WID"
xdotool windowfocus --sync "$WID" 2>&1; sleep 0.3
xdotool windowactivate --sync "$WID" 2>&1; sleep 0.3
echo "active=$(xdotool getactivewindow 2>/dev/null)"
echo "--- send q ---"
xdotool key q; sleep 2
if kill -0 "$APP_PID" 2>/dev/null; then echo "RESULT: STILL ALIVE"; kill "$APP_PID" 2>/dev/null; else echo "RESULT: EXITED (q worked)"; fi
echo "=== tail of event log ==="
tail -8 "$LOG" 2>/dev/null
