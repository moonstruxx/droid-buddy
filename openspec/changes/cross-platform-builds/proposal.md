# Proposal: Windows and macOS builds with a platform-aware runtime

## Why

The app is Linux-only in five places that a shipped binary cannot paper over, and
nothing proves or distributes it anywhere else.

Measured in the tree:

- **User data has no Windows path.** `config.rs:416`, `app.rs:245` and
  `plugin.rs::plugin_dir` each resolve `$XDG_CONFIG_HOME` or `$HOME/.config`.
  Windows sets neither, so `labels_dir` returns `None` and saving fails with
  "cannot determine config directory ($XDG_CONFIG_HOME or $HOME)". No settings,
  no labels, no favourites, no plugins.
- **The faceplates need files the binary does not carry.** `geometry.rs:80` and
  `physical.rs:130` read `rack_geometry.json` and `controller_geometry.json` from
  `CARGO_MANIFEST_DIR` at runtime. Outside the repo `RackGeometry::load` fails
  (the wiring-outlier scan silently switches off) and `ControllerGeometry::load`
  fails, so the module UI falls back to plain widths. This is already broken for
  a Linux binary copied to another machine.
- **MIDI upload cannot find a port off Linux.** Port discovery runs `amidi -l`
  only (`app.rs:688`). macOS and Windows have no `amidi`, so `sendmidi` is found
  but `can_send()` is false because no port was ever listed.
- **`command_present` cannot see a Windows executable.** `app.rs:680` tests
  `dir.join("sendmidi").is_file()`, which is false for `sendmidi.exe`.
- **The picker cannot leave a Windows drive.** It navigates through `parent()`,
  and `C:\` has no parent (`handler.rs:2718`), so other drives are unreachable.

CI runs one job on `ubuntu-latest`, no artifact is ever produced, and the manual
(`docs/config.md`, `docs/labels.md`, `docs/uploading.md`) documents Linux paths
and `amidi` only. `ARCHITECTURE.md` §17 still says "Runtime: native binary;
Linux".

## What Changes

- New `src/platform.rs`: one data-directory resolver. XDG on Linux and macOS
  (unchanged), `%APPDATA%\droid-tui` on Windows. Config, labels, favourites and
  plugins all use it; the three duplicated XDG implementations are deleted.
- `controller_geometry.json` and `rack_geometry.json` are compiled in with
  `include_str!` (schema.rs precedent). The runtime file search and its failure
  paths are removed.
- MIDI transport discovery falls back to `sendmidi list` when `amidi` is
  absent, with a new pure `parse_sendmidi_list` seam, and `command_present`
  resolves platform executable extensions on Windows. The confirm modal and the
  status keep naming the tool that is actually used.
- The file picker lists the other available drives when it sits at a Windows
  drive root (probe `A:` to `Z:`, no dependency).
- CI gains a matrix: `windows-latest` and `macos-latest` build and run the test
  suite; `ubuntu-latest` keeps fmt, clippy, the full suite, the release build and
  the release perf gate. `tests/perf_gate.rs` is gated to Linux, because its
  budgets are Linux release timings.
- New tag-triggered release workflow on `v*`: a Windows x86_64 zip containing
  `droid_tui.exe`, and a macOS universal `.app` zip (arm64 plus x86_64, joined
  with `lipo`). Both unsigned.
- Documentation: new `docs/platforms.md` and `docs/build.md`; the README gains
  an install section; getting-started, config, labels, uploading and
  contributing are updated for three OSes; `ARCHITECTURE.md` is regenerated.

## Capabilities

### New Capabilities

- `distribution`: the CI verification matrix and the per-platform release
  artifacts.

### Modified Capabilities

- `configuration`: the config file is read from the platform data directory
  (`%APPDATA%\droid-tui` on Windows; XDG unchanged on Linux and macOS), and the
  same resolver serves labels, favourites and plugins.
- `circuit-plugins`: plugin discovery follows the platform data directory.
- `label-management`: the label store follows the platform data directory.
- `file-picker/favourites`: the favourites store follows the platform data
  directory.
- `file-picker`: at a drive root the picker offers the other available drives
  (Windows only).
- `midi-upload`: port discovery works without `amidi` via `sendmidi list`, and
  tool detection resolves platform executable extensions.

## Impact

- New: `src/platform.rs`, `.github/workflows/release.yml`,
  `scripts/package-macos.sh`, `docs/platforms.md`, `docs/build.md`.
- Changed: `src/lib.rs`, `src/config.rs`, `src/app.rs`, `src/favorites.rs`,
  `src/plugin.rs`, `src/geometry.rs`, `src/physical.rs`, `src/gui/picker.rs`,
  `tests/perf_gate.rs`, `.github/workflows/ci.yml`, `README.md`, `docs/*.md`,
  `ARCHITECTURE.md`.
- No new crate dependency. No dependency version change, so `Cargo.lock` stays
  untouched.
- Linux behavior: identical, except the geometry data is compiled in rather than
  read from disk. macOS: unchanged XDG paths, which already work because `HOME`
  is set.
- Windows: new data-directory rule, working faceplates, working MIDI discovery.

## Non-goals

- Code signing and notarization. Both need credentials this repository does not
  have (an Apple developer account, a Windows code-signing certificate); the
  manual documents the Gatekeeper and SmartScreen prompts instead.
- MSI, DMG or `pkg` installers. Plain zips with the executable, or the `.app`
  bundle.
- A native MIDI crate (`midir`). The external `amidi`/`sendmidi` bridge stays,
  so ADR 47 keeps its shape and no dependency is added.
- Windows ARM64, and macOS builds that are not universal.
- Idiomatic macOS paths (`~/Library/Application Support`). XDG already works on
  macOS, so changing it would add a migration for no gain.
- Any UI redesign, any behavior change on Linux other than the embedded geometry.
