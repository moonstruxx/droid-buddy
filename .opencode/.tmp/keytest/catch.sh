#!/usr/bin/env bash
# Catch the focus thief: run droid_tui (Wayland), then snapshot the focused window repeatedly.
set -u
rm -f /tmp/droid_tui_events.log
export DROID_DEBUG_EVENTS=1
./target/debug/droid_tui >/tmp/droid_tui_stdout.log 2>&1 &
APP_PID=$!
echo "app pid=$APP_PID"
# helper: dump focused window(s) via gnome-shell introspection
dump() {
  echo "--- t=$1 ---"
  busctl --user call org.gnome.Shell /org/gnome/Shell/Introspect org.gnome.Shell.Introspect GetWindows 2>&1 \
    | tr '}' '\n' \
    | grep -oE '"(title|has-focus|app-id|wm-class)":\s*("[^"]*"|true|false)' \
    | sed 's/^/    /'
}
for i in $(seq 1 12); do
  dump "${i}0" | head -0
  # Only print the focused line
  snapshot=$(busctl --user call org.gnome.Shell /org/gnome/Shell/Introspect org.gnome.Shell.Introspect GetWindows 2>&1)
  focal=$(echo "$snapshot" | tr '{' '\n' | grep -i 'has-focus' | grep -i 'true')
  title=$(echo "$focal" | grep -oE '"title":\s*"[^"]*"' | head -1)
  app=$(echo "$focal" | grep -oE '"(app-id|wm-class)":\s*"[^"]*"' | head -1)
  echo "t=${i}0  FOCUSED => $title | $app"
  sleep 0.3
done
echo "=== droid_tui stdout ==="
tail -5 /tmp/droid_tui_stdout.log
kill "$APP_PID" 2>/dev/null
