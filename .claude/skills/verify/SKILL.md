---
name: verify
description: Build and drive the droid_tui binary interactively to verify a change. Use when asked to run/verify/smoke-test the native winit/egui window rather than just run its test suite.
---

# Verifying droid_tui (native winit/egui window)

The app is a native desktop window (winit + egui + wgpu). There is no
terminal UI anymore: the old multiplexer-based harness, fixed text-grid
sizes, pty probes, and raw terminal mouse escapes do not apply. Drive the
window with xdotool and capture it with ImageMagick.

Build first: `cargo build`. For live verification prefer the release
binary (`cargo build --release`) — debug frame timing differs. The
one-command gate for native changes is `scripts/verify-native-change.sh`
(fmt, clippy `-D warnings`, locked tests, release build).

## Launching and finding the window

```bash
env -u WAYLAND_DISPLAY WINIT_UNIX_BACKEND=x11 DISPLAY=:0 ./target/release/droid_tui <patch> &
sleep 1
W=$(xdotool search --name droid_tui | tail -1)
xdotool windowfocus --sync "$W"   # returns rc 0 on this setup
```

Notes:

- Force the X11 backend as shown. A Wayland-native window cannot be
  enumerated or driven by xdotool at all.
- `xdotool windowactivate --sync` times out (~10 s) on this machine; use
  `windowfocus --sync` instead.
- The xdg-desktop-portal RemoteDesktop harness
  (`.opencode/.tmp/keytest/rpc_keys.py`) needs an interactive permission
  dialog and blocks without it — prefer the xdotool path.

## Keyboard injection

```bash
xdotool key --window "$W" g
sleep 0.2
xdotool key --window "$W" g   # g g opens the signal-flow graph
sleep 1
```

- Focus first (`windowfocus`), then `key --window`. An unfocused window
  ignores keys, and bare global `xdotool key` is ignored on this setup
  (XTEST goes nowhere) — always use `key --window`.
- If `key --window` stops landing (e.g. `q` doesn't quit), the fallback is
  kernel-level injection via a python-evdev UInput virtual keyboard
  (`/dev/uinput` is writable, `evdev` is installed): `UInput({EV_KEY:[...]})`,
  press with EV_KEY 1/syn/0/syn per key.
- Surface chords: `g v` source viewer, `g g` signal-flow graph, `g d`
  diff-picker, `g o` latency optimizer, `g s` select-state menu,
  `g c` latency coloring, `l` file picker, `e` validation modal / label
  editor, `?` help. `q` / Ctrl+C quits.

## Pointer warp pitfall (X11 scale 2) — calibrate, don't assume

`xdotool mousemove` takes LOGICAL screen coordinates but the pointer lands
at 2x PHYSICAL (`mousemove 300 300` → `getmouselocation` reports 600,600),
and the `--window` form is unreliable: it has been observed both
double-applying the window origin (landing at
`2*window_pos + 2*(x,y)`) and acting as a complete no-op, depending on
setup. Never assume a formula. Calibrate per session:

```bash
xdotool getwindowgeometry "$W"   # physical pos + size; logical = physical / 2
xdotool mousemove <X> <Y>        # absolute (root) logical coords
xdotool getmouselocation         # VERIFY the pointer actually moved
```

- To hit a window-relative logical point (lx,ly): start from
  `(win_phys_x/2 + lx, win_phys_y/2 + ly)`, move, read back
  `getmouselocation`, and adjust. One calibration round is enough per
  window position; re-calibrate after moving the window.
- A hover needs the pointer exactly on the cell. The warp is flaky —
  always confirm with `getmouselocation` before clicking or capturing.

## Capture and diff

```bash
import -window "$W" before.png   # ImageMagick; reflects real wgpu frames
# ... drive the UI ...
import -window "$W" after.png
compare -metric AE before.png after.png null:   # changed-pixel count
```

- `grim` and ffmpeg x11grab do not work here (no screen-capture protocol /
  black XWayland root). Use `import -window`.
- A full-window grab takes ~0.15 s (.ppm) / ~0.4 s (.png).

## Proof standard

- Reproduce against the real window at a representative size (window is
  2560x1600 physical = 1280x800 logical on this machine).
- Capture each key's effect in a state where it is actually visible —
  verify zoom/scale first (deep graph zoom renders cables sub-pixel and
  hides edge/color/glyph changes).
- Order irreversible or state-changing keys so the needed capture precedes
  later state changes.
- A zero-diff capture is missing proof, not a pass: re-check focus,
  pointer position (`getmouselocation`), and zoom before concluding
  anything. (Rapid key bursts also need ~1 s to settle before capturing.)
