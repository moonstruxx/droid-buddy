#!/usr/bin/env bash
set -u
export OUT=/home/bjoern/projects/droid_tui/.opencode/.tmp/keytest
rm -f /tmp/droid_tui_events.log "$OUT"/shot_*.png
env -u WAYLAND_DISPLAY DROID_DEBUG_EVENTS=1 ./target/debug/droid_tui >/tmp/dt_stdout.log 2>&1 &
APP=$!
WID=""
for i in $(seq 1 40); do
  kill -0 $APP 2>/dev/null || { echo "APP DIED"; cat /tmp/dt_stdout.log; exit 1; }
  WID=$(xdotool search --name "droid_tui" 2>/dev/null | tail -1)
  [ -n "$WID" ] && break; sleep 0.25
done
echo "app=$APP wid=$WID"
xdotool windowfocus --sync $WID >/dev/null 2>&1
sleep 1.5
import -window "$WID" "$OUT/shot_1_startup.png" 2>&1 | head -2
echo "startup captured"
# send l (picker) via XSendEvent to the window directly
xdotool key --window $WID l >/dev/null 2>&1; sleep 1.2
import -window "$WID" "$OUT/shot_2_after_l.png" 2>&1 | head -2
echo "after-l captured"
# send backslash (quad toggle?) 
xdotool key --window $WID backslash >/dev/null 2>&1; sleep 1.2
import -window "$WID" "$OUT/shot_3_after_bslash.png" 2>&1 | head -2
echo "after-\\ captured"
kill $APP 2>/dev/null; sleep 0.3; pkill -x droid_tui 2>/dev/null
python3 - <<'PY'
from PIL import Image
import glob, os
for f in sorted(glob.glob(os.path.expanduser('/home/bjoern/projects/droid_tui/.opencode/.tmp/keytest/shot_*.png'))):
    im = Image.open(f).convert('RGB')
    px = im.load(); w,h = im.size
    nonblack = sum(1 for y in range(0,h,4) for x in range(0,w,4) if sum(px[x,y])>30)
    colors = len(im.getcolors(maxcolors=100000) or [])
    print(f"{os.path.basename(f)}: {w}x{h} nonblack_sampled={nonblack} distinct_colors={colors}")
PY
echo "=== keys seen in event log ==="
grep -c "KeyboardInput" /tmp/droid_tui_events.log 2>/dev/null || echo 0
grep "KeyboardInput" /tmp/droid_tui_events.log 2>/dev/null | head -5
