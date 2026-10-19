# Platforms

`droid_tui` builds and runs on Linux, Windows and macOS from the same
source. Linux is the original platform and the one the performance
budgets are calibrated on; the Windows and macOS builds are newer and
share the same UI and file handling.

This page is the platform reference: what works where, where your
files live, what a MIDI upload needs, and the limits you should know
before you download a build. New to the app? Start with
[Getting started](getting-started.md).

## Support

| Capability | Linux | Windows | macOS |
|---|---|---|---|
| Load and inspect patches, all views | yes | yes | yes |
| Config, labels, favourites, plugins | yes | yes | yes |
| MIDI SysEx upload | yes, with `amidi` or `sendmidi` | yes, with `sendmidi` | yes, with `sendmidi` |
| Other drives in the picker | no | yes, at a drive root | no |
| Prebuilt download | none, build from source | x86_64 zip with `droid_tui.exe` | universal `.app` zip |
| Release performance gate | yes | not run | not run |

### What CI proves

CI compiles the crate and runs the test suite on Linux, Windows and
macOS for every push and pull request. Only the Linux job also runs
`cargo fmt --check`, clippy, the release build and the release
performance gate; the gate's budgets are Linux release timings, so
running it on a shared Windows or macOS runner would report noise.

What CI cannot do: it never opens a window and never touches a MIDI
device. So a green run proves the crate builds and its tests pass on
each target — not that the window paints correctly, that a send
reaches hardware, or that the downloaded artifact starts on a machine
without the source tree. Those stay manual smoke tests, and are the
first things to check when trying a new platform build. The build and
release commands are on [Building from source](build.md).

## Where your files live

All user data lives in one directory, resolved by one rule:

| | Linux | macOS | Windows |
|---|---|---|---|
| Data directory | `$XDG_CONFIG_HOME/droid-tui/`, else `~/.config/droid-tui/` | `$XDG_CONFIG_HOME/droid-tui/`, else `~/.config/droid-tui/` | `%APPDATA%\droid-tui\` |

Inside it:

| Path | Holds |
|---|---|
| `config.toml` | theme, labels policy, latency costs, layout, rack rows, plugins |
| `labels.toml` | per-patch, per-shift-layer control and circuit labels |
| `favourites.toml` | pinned files and directories in the picker |
| `plugins/` | user `*.toml` circuit definitions (`[[circuit]]` files) |

By default the four are beside each other — the only exception is an
explicit absolute `[plugins] dir` in `config.toml` — so moving a setup
between Linux and macOS means copying one directory. Their formats are
on [Configuration](config.md) and [Labels](labels.md).

Rules worth knowing:

- `$XDG_CONFIG_HOME` must be an absolute path. An empty or relative
  value is treated as unset and `~/.config` is used.
- On Windows, `%APPDATA%` wins when it is set. If it is empty or
  unset — a MSYS/Cygwin-style shell, for example — the resolver falls
  back to `$HOME/.config` so the app still finds a writable home.
- A missing directory is not an error: the app starts with built-in
  defaults. Only a *save* with no resolvable data directory reports
  that it cannot determine one.
- macOS deliberately keeps the XDG path instead of
  `~/Library/Application Support`. `HOME` is set on macOS, XDG already
  works there, and one rule for Linux and macOS avoids a migration and
  a second path to document.

## MIDI upload prerequisites

The app does not link a MIDI library. It shells out to a command-line
tool that must be on your `PATH`, and it only detects what it can
find there.

| OS | Tool | How to get it |
|---|---|---|
| Linux | `amidi` | part of `alsa-utils`: `sudo apt install alsa-utils` (Debian/Ubuntu), `sudo dnf install alsa-utils` (Fedora), `sudo pacman -S alsa-utils` (Arch) |
| macOS | `sendmidi` | download from the [SendMIDI releases](https://github.com/gbevin/SendMIDI/releases) and put the binary on your `PATH` |
| Windows | `sendmidi.exe` | same download; put `sendmidi.exe` on your `PATH` |

On Windows the presence check accepts `sendmidi.exe`, `sendmidi.cmd`
and `sendmidi.bat`, so a `sendmidi.exe` install counts as `sendmidi`.
On Linux your user must be able to open the raw MIDI device. A desktop
session normally grants that; on a minimal setup, add yourself to the
`audio` group and log in again.

### Discovery order

1. If `amidi` is present, ports come from `amidi -l` and look like
   `hw:1,0`.
2. Otherwise, if `sendmidi` is present, ports come from
   `sendmidi list`, one device name per line.
3. If neither tool is present, there are no ports, the confirm modal
   offers no `y` path, and the status names the missing tool.

`amidi` wins when both exist; it is the native ALSA path on Linux.

**The two tools do not speak the same port language.** An `amidi` port
is an ALSA token (`hw:1,0`); a `sendmidi` port is a device name
(`DROID`, `X7 MIDI 1`, …). The app passes the string through to
whichever tool it uses, so the confirm modal and the status bar name
the transport — `USB-MIDI hw:1,0 via amidi` or `DROID MIDI device
X7 MIDI 1 via sendmidi` — and that is where to look when a port is
not the one you expected.

The sends themselves are `amidi -p <port> -s <file>` and
`sendmidi dev <port> syf <file>`, invoked directly with no shell. See
[Uploading](uploading.md) for the flow, the confirm modal, and what
the app does and does not read back.

## Unsigned builds

Neither release artifact is code-signed or notarized. That is a
deliberate limit, not an oversight: signing needs an Apple developer
account and a Windows code-signing certificate, and this project has
neither. Expect a first-launch warning on both platforms.

**macOS (Gatekeeper).** A downloaded `.app` is quarantined. Right-click
(or Control-click) the app and choose **Open**, then confirm **Open**
in the dialog. If that route is unavailable on your macOS version, use
**System Settings → Privacy & Security** and allow the app under the
"was blocked" notice. Alternatively, remove the quarantine attribute
after unzipping:

```sh
xattr -d com.apple.quarantine /path/to/droid_tui.app
```

**Windows (SmartScreen).** Running `droid_tui.exe` from a downloaded
zip shows "Windows protected your PC". Choose **More info**, then
**Run anyway**. The zip itself needs no special handling; Windows may
also mark the extracted file as blocked, which the same dialog clears.

Nothing here bypasses a real security decision: if you did not get the
file from this repository's releases, do not run it.

## Known limits

- **No native file dialog.** `l` opens the app's own picker, with
  favourites, `..` navigation, and a filter you type into. There is no
  OS "Open with" integration; pass a path on the command line instead
  (`droid_tui patch.ini`, `droid_tui.exe patch.ini`).
- **Windows drive rows only at a drive root.** The picker lists the
  other existing drives (`D:\`, `E:\`, …) when it sits at `C:\`, not
  in every directory. Navigate up to the root with `..` first. On
  Linux and macOS the filesystem root behaves as it always has: no
  parent entry, no drive entries.
- **No code signing or notarization.** Both artifacts trip the
  first-launch prompts described above. There is no signed download.
- **No MSI, DMG or `pkg` installers.** Windows is a plain zip
  containing `droid_tui.exe`; macOS is a universal `.app` bundle in a
  zip. There is no Start Menu entry, no uninstaller and no `PATH`
  entry added for you — deleting the unzipped program is the
  uninstall.
- **Platform reach is narrow.** The Windows build is x86_64 only, and
  the macOS build is a single universal bundle (arm64 plus x86_64).
  Windows ARM64 is not built.
- **No bundled MIDI library.** Upload depends on `amidi` or
  `sendmidi` as described above; if your distribution does not ship
  `alsa-utils`, install it or use `sendmidi` instead.
- **The performance gate is Linux-only.** Its numbers describe a Linux
  release build and are not comparable across platforms.

## See also

- [Getting started](getting-started.md) — install, open a patch, first
  five minutes.
- [Building from source](build.md) — prerequisites per OS, the gates,
  CI jobs and the release process.
- [Configuration](config.md) — every `config.toml` key.
- [Labels](labels.md) — the label store and its storage rules.
- [Uploading](uploading.md) — the SysEx send, step by step.
