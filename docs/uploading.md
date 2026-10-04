# Uploading to hardware

`droid_tui` is a patch viewer with a one-way hardware bridge: it can
**send** the in-memory patch to DROID hardware over MIDI SysEx. It
reads nothing back — component state stays simulated.

## Flow

1. Press `U` (`Shift+u`). A confirm modal shows the byte count and
   the transport.
2. Press `y` to send, `n` or `Esc` to cancel.
3. A waiting bar tracks the in-flight send; the status bar reports
   the verdict. `q`/`Ctrl+c` aborts an in-flight send before quitting.

## Transports

- **USB-MIDI**: auto-detected (X7 probe via `amidi -l`).
- **DIN MIDI**: explicit port.

The send shells out to `amidi -p <port> -s` (preferred) or
`sendmidi dev <port> syf` — directly, no shell. If the tool or the
hardware is missing, the modal names what is missing instead of
silently skipping.

## Payload

The payload is built from the rendered in-memory patch: comments and
non-7-bit content are stripped per the `droidpatch` rules, then framed
as `F0 00 66 66 50 … F7`. The staged `.syx` tempfile is removed after
the send (or on quit), and the child process is reaped on completion
and killed on quit.

Firmware updates, SD-card flows, and rack-to-app transfer remain
future work.
