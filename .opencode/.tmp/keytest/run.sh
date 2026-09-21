#!/usr/bin/env bash
# Controlled live key test: launch on X11, inject keys, inspect the event log.
set -u
LOG=/tmp/droid_tui_events.log
rm -f "$LOG"
export WINIT_UNIX_BACKEND=x11
export DROID_DEBUG_EVENTS=1
./target/debug/droid_tui >/tmp/droid_tui_stdout.log 2>&1 &
APP_PID=$!
echo "pid=$APP_PID"
for i in $(seq 1 40); do
  if ! kill -0 "$APP_PID" 2>/dev/null; then echo "DIED early after ${i}00ms"; cat /tmp/droid_tui_stdout.log; exit 1; fi
  WID=$(xdotool search --name "droid_tui" 2>/dev/null | head -1)
  [ -n "$WID" ] && break
  sleep 0.25
done
echo "wid=$WID"
sleep 1
xdotool windowactivate --sync "$WID" 2>&1 | head -2
sleep 0.5
echo "--- sending q ---"
xdotool key --window "$WID" q 2>&1 | head -2
sleep 1.2
if kill -0 "$APP_PID" 2>/dev/null; then echo "STILL ALIVE after q"; kill "$APP_PID" 2>/dev/null; sleep 0.3; else echo "EXITED after q"; fi
echo "=== EVENT LOG ==="
cat "$LOG" 2>/dev/null || echo "(no log)"
echo "=== STDOUT ==="
tail -5 /tmp/droid_tui_stdout.log
