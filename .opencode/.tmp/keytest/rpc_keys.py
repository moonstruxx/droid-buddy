#!/usr/bin/env python3
"""Minimal xdg-desktop-portal RemoteDesktop key injector.

Creates a RemoteDesktop session, selects Keyboard+Pointer, starts it, then
injects keys via NotifyKeyboardKeysym. A permission dialog ("screen sharing /
remote desktop") will pop; the session blocks until it is answered.

Keys are injected into whatever surface the compositor currently has focused.

Usage: rpc_keys.py <key...>
  keys are arg names: q l g v j k 1..9 ? e d s o Return Escape Tab Up Down space

The first invocation will ask permission; approve the dialog when it appears.
"""

import sys
import time

import dbus
import dbus.mainloop.glib
from gi.repository import GLib

dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)

PORTAL = "org.freedesktop.portal.Desktop"
OPATH = "/org/freedesktop/portal/desktop"
RD = "org.freedesktop.portal.RemoteDesktop"
REQ = "org.freedesktop.portal.Request"
SESSION = "org.freedesktop.portal.Session"

# X11 keysyms (a subset).
KEYSYMS = {
    "Return": 0xFF0D, "Escape": 0xFF1B, "Tab": 0xFF09,
    "Up": 0xFF52, "Down": 0xFF54, "Left": 0xFF51, "Right": 0xFF53,
    "space": 0x20, "BackSpace": 0xFF08,
}
for _c in "abcdefghijklmnopqrstuvwxyz0123456789":
    KEYSYMS[_c] = ord(_c)
KEYSYMS["?"] = ord("?")
KEYSYMS["+"] = ord("+")
KEYSYMS["-"] = ord("-")

bus = dbus.SessionBus()
portal = bus.get_object(PORTAL, OPATH)
sender = bus.get_unique_name()[1:].replace(".", "_")


def wait_response(token, timeout_ms=180000):
    """Block until the request object for `token` emits Response, return (code, results)."""
    outcome = [None]
    loop = GLib.MainLoop()

    def on_response(response, results):
        outcome[0] = (int(response), results)
        loop.quit()

    req_path = f"{OPATH}/request/{sender}/{token}"
    req = bus.get_object(PORTAL, req_path)
    req.connect_to_signal("Response", on_response, dbus_interface=REQ)
    GLib.timeout_add(timeout_ms, loop.quit)
    loop.run()
    if outcome[0] is None:
        raise RuntimeError(f"timeout waiting for {token} (is the dialog still up?)")
    return outcome[0]


def now():
    return int(time.time() * 1000) % 0xFFFFFFFF


def options(**kw):
    d = dbus.Dictionary({k: dbus.String(v) if isinstance(v, str) else v
                         for k, v in kw.items()}, signature="sv")
    return d


def create_session():
    handle_token = f"dtt{now()}"
    session_token = f"dts{now()}"
    portal.CreateSession(
        dbus.Dictionary({
            "handle_token": dbus.String(handle_token),
            "session_handle_token": dbus.String(session_token),
        }, signature="sv"),
        dbus_interface=RD,
    )
    code, results = wait_response(handle_token)
    if code != 0:
        raise RuntimeError(f"CreateSession denied: code={code}")
    return str(results["session_handle"])


def select_devices(session):
    handle_token = f"dtsel{now()}"
    portal.SelectDevices(session, options(
        types=dbus.UInt32(1 | 2),
        handle_token=handle_token,
    ), dbus_interface=RD)
    code, _ = wait_response(handle_token)
    if code != 0:
        raise RuntimeError(f"SelectDevices denied: code={code}")


def start(session):
    handle_token = f"dtst{now()}"
    portal.Start(session, "", options(handle_token=handle_token), dbus_interface=RD)
    code, _ = wait_response(handle_token, timeout_ms=20000)
    if code != 0:
        raise RuntimeError(f"Start denied: code={code}")


def inject(session, keysym, press):
    portal.NotifyKeyboardKeysym(
        session, dbus.Dictionary({}, signature="sv"),
        dbus.Int32(keysym), dbus.UInt32(1 if press else 0),
        dbus_interface=RD,
    )


def tap(session, name):
    ks = KEYSYMS[name]
    inject(session, ks, True)
    time.sleep(0.05)
    inject(session, ks, False)


def close(session):
    try:
        portal.Close(session, dbus_interface=SESSION)
    except Exception:
        pass


def main():
    keys = sys.argv[1:]
    if not keys:
        sys.exit("usage: rpc_keys.py <key...>")
    session = None
    for attempt in range(1, 4):
        try:
            session = create_session()
            print(f"[rpc] attempt {attempt} session={session}", flush=True)
            select_devices(session)
            start(session)
            break
        except RuntimeError as e:
            print(f"[rpc] attempt {attempt} failed: {e}", flush=True)
            if session is not None:
                close(session)
            session = None
            time.sleep(1)
    if session is None:
        sys.exit("all session attempts failed")
    try:
        time.sleep(0.3)
        for k in keys:
            if k not in KEYSYMS:
                sys.exit(f"unknown key {k!r}")
            tap(session, k)
            time.sleep(0.15)
            print(f"[rpc] sent {k}", flush=True)
        time.sleep(0.3)
    finally:
        close(session)
        print("[rpc] session closed", flush=True)


if __name__ == "__main__":
    main()
