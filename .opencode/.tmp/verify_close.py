#!/usr/bin/env python3
"""Verify that closing the window exits the process (droid_tui-7y5).
Uses xdotool to send keys and close the window on X11."""
import os, time, subprocess, sys

binary = "target/release/droid_tui"
if not os.path.exists(binary):
    print("Build the release binary first: cargo build --release")
    sys.exit(1)

env = os.environ.copy()
env.pop("WAYLAND_DISPLAY", None)
env["DISPLAY"] = ":0"

# Start the app
proc = subprocess.Popen(
    [binary],
    env=env,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
print(f"Started process PID {proc.pid}")

# Wait for window to appear
time.sleep(2.0)

# Find the window
result = subprocess.run(
    ["xdotool", "search", "--pid", str(proc.pid)],
    capture_output=True, text=True, env=env
)
windows = result.stdout.strip().split("\n") if result.stdout.strip() else []
print(f"Found {len(windows)} window(s): {windows}")

if not windows:
    print("No window found — app may not have started")
    proc.kill()
    proc.wait()
    sys.exit(1)

wid = windows[0]

# Focus the window first
print(f"Focusing window {wid}...")
subprocess.run(["xdotool", "windowfocus", "--sync", wid], env=env)
time.sleep(0.3)

# Test 1: Send 'q' to quit
print(f"Sending 'q' to window {wid}...")
subprocess.run(["xdotool", "key", "--window", wid, "q"], env=env)
try:
    proc.wait(timeout=3)
    print(f"PASS: Process exited with code {proc.returncode}")
except subprocess.TimeoutExpired:
    print("FAIL: Process did not exit after 'q'")
    proc.kill()
    proc.wait()
    sys.exit(1)
