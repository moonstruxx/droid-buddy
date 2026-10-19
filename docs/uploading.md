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

## Prerequisites

`droid_tui` does not link a MIDI library. It shells out to a
command-line tool that has to be on your `PATH`, and it only finds what
is there.

| OS | Tool | How to get it |
|---|---|---|
| Linux | `amidi` | part of `alsa-utils` — `sudo apt install alsa-utils` on Debian/Ubuntu, `sudo dnf install alsa-utils` on Fedora, `sudo pacman -S alsa-utils` on Arch. `sendmidi` also works. |
| macOS | `sendmidi` | download from the [SendMIDI releases](https://github.com/gbevin/SendMIDI/releases) and put the binary on your `PATH`. |
| Windows | `sendmidi.exe` | same download; `sendmidi.exe` on `PATH` counts as `sendmidi`. `sendmidi.cmd` and `sendmidi.bat` are accepted too. |

On Linux your user must be able to open the raw MIDI device; a desktop
session normally grants that. [Platforms → MIDI upload
prerequisites](platforms.md#midi-upload-prerequisites) has the group
advice and the per-distribution packages.

## Transports

Ports are discovered in this order:

1. `amidi` present → `amidi -l`. Ports look like `hw:1,0`.
2. Otherwise `sendmidi` present → `sendmidi list`, one device name per
   line.
3. Neither present → no ports, and the modal names the missing tool.

`amidi` wins when both exist; it is the native ALSA path on Linux.

- **USB-MIDI**: auto-detected (the DROID/X7 entry in the port list).
- **DIN MIDI**: explicit port.

**The two tools do not share a port vocabulary.** An `amidi` port is an
ALSA token (`hw:1,0`); a `sendmidi` port is a device name (`DROID`,
`X7 MIDI 1`). The app passes the string straight to whichever tool it
uses, so the confirm modal and status bar name the tool — `USB-MIDI
hw:1,0 via amidi` versus `DROID MIDI device X7 MIDI 1 via sendmidi` —
and that is the line to read when the port is not the one you expected.

The send is `amidi -p <port> -s <file>` (preferred) or `sendmidi dev
<port> syf <file>` — directly, no shell. If the tool or the hardware is
missing, the modal names what is missing instead of silently skipping.

## Payload

The payload is built from the rendered in-memory patch: comments and
non-7-bit content are stripped per the `droidpatch` rules, then framed
as `F0 00 66 66 50 … F7`. The staged `.syx` tempfile is removed after
the send (or on quit), and the child process is reaped on completion
and killed on quit.

Firmware updates, SD-card flows, and rack-to-app transfer remain
future work.
