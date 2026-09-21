#!/usr/bin/env python3
"""Live visual proof driver: one RemoteDesktop session injects the surface-open
keys while the app runs under Xwayland and `import` captures each surface."""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rpc_keys

ROOT = "/home/bjoern/projects/droid_tui"
OUT = f"{ROOT}/.opencode/.tmp/keytest/verify"
PATCH = sys.argv[1] if len(sys.argv) > 1 else f"{ROOT}/fixtures/arpeggio1.ini"
os.makedirs(OUT, exist_ok=True)

# Surfaces in open order. (name, [open keys], [close keys])
SHOTS = [
    ("help", ["?"], ["Escape"]),
    ("picker", ["l"], ["Escape"]),
    ("viewer", ["g", "v"], ["Escape"]),
    ("graph", ["g", "g"], ["Escape"]),
    ("optimizer", ["g", "o"], ["Escape"]),
]


def find_window(timeout=6.0):
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
        time.sleep(0.5)
    return None


def shot(window_id, name):
    png = f"{OUT}/{name}.png"
    subprocess.run(["import", "-window", window_id, png], capture_output=True, text=True, timeout=10)
    jpg = f"{OUT}/{name}.jpg"
    subprocess.run(["magick", png, "-resize", "1400x", "-quality", "85", jpg],
                   capture_output=True, text=True, timeout=20)
    return png


def main():
    env = dict(os.environ)
    env.pop("WAYLAND_DISPLAY", None)  # force X11/Xwayland so `import` can see it
    app = subprocess.Popen(
        [f"{ROOT}/target/release/droid_tui", PATCH],
        stdout=open("/tmp/dt_verify.log", "wb"), stderr=subprocess.STDOUT, env=env,
    )
    print(f"[driver] app pid={app.pid}", flush=True)
    window_id = find_window()
    if not window_id:
        print("[driver] no window found", flush=True)
        app.kill(); return 1
    print(f"[driver] window={window_id}", flush=True)

    session = rpc_keys.create_session()
    print(f"[driver] session={session}", flush=True)
    try:
        rpc_keys.select_devices(session)
        rpc_keys.start(session)
        time.sleep(0.5)
        shot(window_id, "baseline")
        print("[driver] baseline captured", flush=True)

        for name, open_keys, close_keys in SHOTS:
            for k in open_keys:
                rpc_keys.tap(session, k)
                time.sleep(0.2)
            time.sleep(1.5)
            png = shot(window_id, name)
            print(f"[driver] captured {name} -> {png}", flush=True)
            for k in close_keys:
                rpc_keys.tap(session, k)
                time.sleep(0.2)
            time.sleep(0.6)
    finally:
        rpc_keys.close(session)
    app.terminate()
    time.sleep(1)
    if app.poll() is None:
        app.kill()
    print("[driver] done", flush=True)


if __name__ == "__main__":
    sys.exit(main())
