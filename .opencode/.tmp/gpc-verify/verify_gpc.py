#!/usr/bin/env python3
"""Graph-pane-centering task 4.4: live visual verification.

Proves on the running windowed app (Xwayland):
  c       -> graph centers in the visible pane
  C       -> Shift+c refits and centers (zoom preset -> fit index, 100%)
  - x6    -> zoom-out reaches 0.03125 (3%), below the old 0.0625 floor
  g c     -> toggles latency coloring

Evidence per step: screenshot (hardcopy), pixel-diff stats vs previous state,
and (best effort) the AT-SPI accessibility text of the window.
"""
import os
import re
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "keytest"))
import rpc_keys  # noqa: E402

# The harness keysym table lacks uppercase; add the Shift+c keysym.
rpc_keys.KEYSYMS["C"] = ord("C")

# Start can wait on a permission dialog; give the user time to approve it.
def _start_long(session):
    handle_token = f"dtst{int(time.time() * 1000) % 0xFFFFFFFF}"
    rpc_keys.portal.Start(
        session, "", rpc_keys.options(handle_token=handle_token),
        dbus_interface=rpc_keys.RD,
    )
    code, _ = rpc_keys.wait_response(handle_token, timeout_ms=180000)
    if code != 0:
        raise RuntimeError(f"Start denied: code={code}")


rpc_keys.start = _start_long

ROOT = "/home/bjoern/projects/droid_tui"
OUT = f"{ROOT}/.opencode/.tmp/gpc-verify/shots"
PATCH = sys.argv[1] if len(sys.argv) > 1 else f"{ROOT}/fixtures/arpeggio1.ini"
BIN = sys.argv[2] if len(sys.argv) > 2 else f"{ROOT}/target/release/droid_tui"
os.makedirs(OUT, exist_ok=True)


def find_window(timeout=8.0):
    t0 = time.time()
    while time.time() - t0 < timeout:
        try:
            out = subprocess.run(
                ["xdotool", "search", "--name", "droid_tui"],
                capture_output=True, text=True, timeout=3,
            ).stdout.strip().splitlines()
            if out:
                return out[-1]
        except Exception:
            pass
        time.sleep(0.4)
    return None


def shot(window_id, name):
    png = f"{OUT}/{name}.png"
    subprocess.run(["import", "-window", window_id, png],
                   capture_output=True, text=True, timeout=15)
    return png


def atspi_text():
    """Best-effort dump of all accessible text in the droid_tui window."""
    try:
        from gi.repository import Atspi
        texts = []
        desktop = Atspi.get_desktop(0)

        def walk(obj, depth=0):
            try:
                role = obj.get_role_name() or ""
                name = obj.get_name() or ""
                if role in ("label", "static", "text", "push button", "panel"):
                    try:
                        t = obj.get_text(0, -1)
                        if t and t.strip():
                            texts.append(f"{'  '*depth}[{role}] {t.strip()}")
                    except Exception:
                        pass
                n = obj.get_child_count()
                for i in range(n):
                    try:
                        walk(obj.get_child_at_index(i), depth + 1)
                    except Exception:
                        pass
            except Exception:
                pass

        walk(desktop)
        return "\n".join(texts[:200])
    except Exception as e:
        return f"(atspi unavailable: {e})"


def pix_stats(png):
    """Nonblack sampled pixels + dominant-color signature for change detection."""
    try:
        from PIL import Image
        im = Image.open(png).convert("RGB")
        px = im.load()
        w, h = im.size
        nonblack = 0
        hist = {}
        for y in range(0, h, 3):
            for x in range(0, w, 3):
                r, g, b = px[x, y]
                if r + g + b > 60:
                    nonblack += 1
                    key = (r // 32, g // 32, b // 32)
                    hist[key] = hist.get(key, 0) + 1
        top = sorted(hist.items(), key=lambda kv: -kv[1])[:5]
        return w, h, nonblack, [(f"#{k[0]*32:02x}{k[1]*32:02x}{k[2]*32:02x}", v) for k, v in top]
    except Exception as e:
        return f"(pix error: {e})"


def diff_between(a, b):
    """Bounding box + count of differing pixels between two captures."""
    try:
        from PIL import Image, ImageChops
        ia = Image.open(a).convert("RGB")
        ib = Image.open(b).convert("RGB")
        diff = ImageChops.difference(ia, ib)
        bbox = diff.getbbox()
        changed = sum(1 for p in diff.getdata() if sum(p) > 40)
        return bbox, changed
    except Exception as e:
        return f"(diff error: {e})"


def main():
    env = dict(os.environ)
    env.pop("WAYLAND_DISPLAY", None)  # force X11/Xwayland so `import` sees the window
    log = open(f"{OUT}/driver.log", "wb")
    app = subprocess.Popen([BIN, PATCH], stdout=log, stderr=subprocess.STDOUT, env=env)
    print(f"[driver] app pid={app.pid} bin={BIN}", flush=True)
    window_id = find_window()
    if not window_id:
        print("[driver] NO WINDOW", flush=True)
        app.kill()
        return 1
    print(f"[driver] window={window_id}", flush=True)
    subprocess.run(["xdotool", "windowactivate", "--sync", window_id],
                   capture_output=True, text=True, timeout=10)
    time.sleep(1.5)

    session = rpc_keys.create_session()
    print(f"[driver] rdp session={session}", flush=True)
    try:
        rpc_keys.select_devices(session)
        rpc_keys.start(session)
        time.sleep(0.5)

        steps = [
            ("0_baseline", []),
            ("1_gg_graph_open", ["g", "g"]),
            ("2_c_center", ["c"]),
            ("3_C_shiftc_fit", ["C"]),
            ("4_minus_x6_zoomout", ["-", "-", "-", "-", "-", "-"]),
            ("5_gc_latency", ["g", "c"]),
        ]
        for name, keys in steps:
            for k in keys:
                rpc_keys.tap(session, k)
                time.sleep(0.25)
            time.sleep(1.6)
            png = shot(window_id, name)
            print(f"[driver] {name} -> {png}", flush=True)
            print(f"[driver]   pix {pix_stats(png)}", flush=True)
            if name != "0_baseline":
                prev = f"{OUT}/{steps[steps.index((name, keys)) - 1][0]}.png"
                print(f"[driver]   diff-vs-prev {diff_between(prev, png)}", flush=True)
            txt = atspi_text()
            print(f"[driver]   atspi[{len(txt)}]: {txt[:600]}", flush=True)
    finally:
        rpc_keys.close(session)
    app.terminate()
    time.sleep(1)
    if app.poll() is None:
        app.kill()
    print("[driver] done", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())