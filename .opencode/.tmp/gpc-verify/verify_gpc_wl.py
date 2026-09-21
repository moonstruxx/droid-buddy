#!/usr/bin/env python3
"""Graph-pane-centering task 4.4, Wayland-native variant.

The Xwayland variant failed: the RemoteDesktop Start dialog cannot associate
with an Xwayland window ("Failed to associate portal window with parent
window"). Running natively on Wayland makes the window a first-class mutter
window: the dialog binds, keys inject into the focused window, and captures
use org.gnome.Shell.Screenshot.ScreenshotWindow (the window has startup focus).
"""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "keytest"))
import rpc_keys  # noqa: E402

rpc_keys.KEYSYMS["C"] = ord("C")


def _start_long(session):
    handle_token = f"dtst{int(time.time() * 1000) % 0xFFFFFFFF}"
    rpc_keys.portal.Start(
        session, "", rpc_keys.options(handle_token=handle_token),
        dbus_interface=rpc_keys.RD,
    )
    code, _ = rpc_keys.wait_response(handle_token, timeout_ms=240000)
    if code != 0:
        raise RuntimeError(f"Start denied: code={code}")


rpc_keys.start = _start_long

ROOT = "/home/bjoern/projects/droid_tui"
OUT = f"{ROOT}/.opencode/.tmp/gpc-verify/shots-wl"
PATCH = sys.argv[1] if len(sys.argv) > 1 else f"{ROOT}/fixtures/arpeggio1.ini"
BIN = sys.argv[2] if len(sys.argv) > 2 else f"{ROOT}/target/release/droid_tui"
os.makedirs(OUT, exist_ok=True)

SHOT = ["gdbus", "call", "--session", "--dest", "org.gnome.Shell",
        "--object-path", "/org/gnome/Shell/Screenshot",
        "--method", "org.gnome.Shell.Screenshot.ScreenshotWindow",
        "false", "false", "false"]


def shot(name):
    png = f"{OUT}/{name}.png"
    r = subprocess.run(SHOT + [png], capture_output=True, text=True, timeout=20)
    ok = "true" in r.stdout
    return png, ok, r.stdout.strip()


def pix_stats(png):
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


def atspi_text():
    try:
        from gi.repository import Atspi
        texts = []
        desktop = Atspi.get_desktop(0)

        def walk(obj, depth=0):
            try:
                role = obj.get_role_name() or ""
                if role in ("label", "static", "text", "push button"):
                    try:
                        t = obj.get_text(0, -1)
                        if t and t.strip():
                            texts.append(t.strip())
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
        return "\n".join(texts[:150])
    except Exception as e:
        return f"(atspi unavailable: {e})"


def main():
    log = open(f"{OUT}/driver.log", "wb")
    app = subprocess.Popen([BIN, PATCH], stdout=log, stderr=subprocess.STDOUT)
    print(f"[driver] app pid={app.pid} (Wayland native)", flush=True)
    time.sleep(3.0)  # startup + window open + focus

    session = rpc_keys.create_session()
    print(f"[driver] rdp session={session}", flush=True)
    try:
        rpc_keys.select_devices(session)
        print("[driver] devices selected; starting (approve dialog if shown)", flush=True)
        rpc_keys.start(session)
        print("[driver] started", flush=True)
        time.sleep(0.8)

        steps = [
            ("0_baseline", []),
            ("1_gg_graph_open", ["g", "g"]),
            ("2_c_center", ["c"]),
            ("3_C_shiftc_fit", ["C"]),
            ("4_minus_x6_zoomout", ["-", "-", "-", "-", "-", "-"]),
            ("5_gc_latency", ["g", "c"]),
        ]
        prev = None
        for name, keys in steps:
            for k in keys:
                rpc_keys.tap(session, k)
                time.sleep(0.25)
            time.sleep(1.8)
            png, ok, resp = shot(name)
            print(f"[driver] {name} shot={ok} resp={resp} -> {png}", flush=True)
            print(f"[driver]   pix {pix_stats(png)}", flush=True)
            if prev:
                print(f"[driver]   diff-vs-prev {diff_between(prev, png)}", flush=True)
            prev = png
            txt = atspi_text()
            print(f"[driver]   atspi[{len(txt)}] {txt[:500]}", flush=True)
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