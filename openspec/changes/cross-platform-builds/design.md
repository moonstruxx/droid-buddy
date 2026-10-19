# Design: Windows and macOS builds with a platform-aware runtime

## Context

The UI stack is already portable: winit 0.30, egui 0.36 and wgpu 30 need no
platform branches, there is no `cfg(target_os)` in `src/`, and
`patch.rs:133` tracks CRLF explicitly. Everything that blocks a Windows or
macOS build is around the edges:

| area | today | consequence off Linux |
| --- | --- | --- |
| user data | `config.rs:416`, `app.rs:245`, `plugin.rs::plugin_dir`, each its own XDG rule | Windows: no directory, saves fail |
| faceplate data | `geometry.rs:80`, `physical.rs:130` read JSON via `CARGO_MANIFEST_DIR` | any machine without the repo loses geometry |
| MIDI ports | `amidi -l` only (`app.rs:688`) | no port on macOS or Windows |
| tool lookup | `dir.join("sendmidi").is_file()` (`app.rs:680`) | `sendmidi.exe` reads as absent |
| picker | `parent()` navigation (`handler.rs:2718`) | Windows drive roots are dead ends |

## Decision 1: one platform data-directory resolver

New `src/platform.rs` exposes a single resolver with the environment injected,
the shape the existing tests already use:

```
data_dir(xdg_config_home, home, appdata) -> Option<PathBuf>
```

Rules:

- Linux, macOS: an absolute non-empty `$XDG_CONFIG_HOME`, else `$HOME/.config`.
  Unchanged, including the "non-absolute is treated as unset" rule.
- Windows: `%APPDATA%`. If it is unset, fall back to `$HOME/.config` so a
  MSYS/Cygwin-style environment still works, then `None`.
- Every consumer appends `droid-tui`: config, labels, favourites, plugins.

`config.rs`, `app.rs` (label store), `favorites.rs` and `plugin.rs` drop their
private copies. That removes three duplicate implementations, which is a
straight DRY win independent of Windows.

## Decision 2: geometry data is compiled in

`rack_geometry.json` (832 B) and `controller_geometry.json` (103 KB) move into
`include_str!`, the precedent set by `circuits.json`. `RackGeometry::load` and
`ControllerGeometry::load` parse the embedded text and keep returning
`Result` so a malformed artifact still degrades with a warning instead of a
panic.

Consequence: changing geometry needs a rebuild. That is already true for
`circuits.json`, so there is one rule for data artifacts instead of two.

## Decision 3: MIDI discovery without `amidi`

Order of preference:

1. `amidi` present: `amidi -l`, ports parsed as `hw:N,M` (Linux today).
2. otherwise `sendmidi` present: `sendmidi list`, one device name per line.

`UploadTransport` already carries `droid_port` plus the tool flags, and
`transport_line()` already renders the tool. The two vocabularies differ, so the
port string keeps meaning "whatever this tool accepts". The send command shapes
are unchanged: `amidi -p <port> -s <file>` and
`sendmidi dev <port> syf <file>`.

Tool detection gains platform executable extensions on Windows
(`.exe`, `.cmd`, `.bat`). Spawning already works there, because Rust's
`Command` resolves `.exe` through the platform loader; only the presence check
was wrong.

## Decision 4: Windows drive roots

`platform::drive_roots(current)` returns the other existing drives. On Windows
it probes `A:` to `Z:` with `Path::exists()` and skips the current one, 25
cheap stat calls and no dependency. The function returns an empty vector
everywhere else, so the picker code has one branch and the Unix tests assert
the empty result.

## Decision 5: CI matrix, perf gate scoped to Linux

```
ubuntu-latest    fmt, clippy, cargo test, release build, release perf gate
windows-latest   build, cargo test
macos-latest     build, cargo test
```

`tests/perf_gate.rs` gets `#![cfg(target_os = "linux")]`. Its budgets are
release timings measured on the anchor fixture here; a shared runner with a
different CPU would make them flaky, and a flaky gate is worse than none. The
`performance-gate` spec only requires the gate to run in release in CI, which
stays true.

fmt and clippy stay on Linux only: they are deterministic across targets, and
duplicating them triples the feedback delay for no extra signal.

## Decision 6: release artifacts

A separate workflow on `v*` tags:

- Windows: `cargo build --release --target x86_64-pc-windows-msvc`, zip
  `droid_tui.exe`.
- macOS: build `aarch64-apple-darwin` and `x86_64-apple-darwin` on the arm
  runner, `lipo -create` them into one binary, assemble
  `droid_tui.app/Contents/{MacOS,Info.plist}` and zip it.

Both unsigned. Signing needs an Apple developer account and a Windows
certificate, neither of which exists here; the manual documents the first-launch
prompts instead of pretending they are absent.

## Decision 7: documentation

The manual already exists (`README.md`, `docs/`). It gains:

- `docs/platforms.md`: support matrix, per-OS data paths, MIDI prerequisites,
  known limits (unsigned artifacts, no native file dialog).
- `docs/build.md`: prerequisites per OS, the four gates, CI jobs, release
  process, artifact layout, how to cut a release.
- Updates: README install section, getting-started (prebuilt download and
  per-OS build), config and labels (paths), uploading (`alsa-utils` versus
  `sendmidi`), contributing (cross-platform gates, perf-gate scoping).
- `ARCHITECTURE.md` is regenerated with `/make-architecture`; it is generated,
  so its "Runtime: Linux" line is not hand-patched.

## Why not the alternatives

- **`dirs` / `directories` crate for the resolver.** One function does not
  justify a dependency, and the crate reads the environment directly, which
  would break the injected-environment test seam the path tests depend on.
- **Idiomatic macOS path (`~/Library/Application Support`).** `HOME` is set on
  macOS, so XDG already works there today. Switching would add a migration and a
  second rule for no functional gain. Documented as a deliberate choice.
- **Shipping the two JSON files beside the executable.** Works, but adds a
  "data files must travel with the binary" rule to every packaging path, and
  still breaks when someone copies just the binary. Compiling them in removes
  the failure mode.
- **Native MIDI via `midir`.** Correct long term, but it rewrites ADR 47, drops
  the "no MIDI dependency" property, and its value here is zero as long as
  `sendmidi` remains the documented transport on macOS and Windows.
- **Signing now.** Cannot be tested without credentials, and an unsigned zip is
  usable with a two-step Gatekeeper/SmartScreen bypass that the docs explain.

## Risks

1. **First `windows-latest` run is unproven.** The known work (19 unix-path
   assertions) is task 7, but other failures may appear: path-join assertions,
   CRLF assumptions in fixtures, or timing-sensitive tests. Task 8 is where
   triage happens; the workflow should land green before task 9.
2. **No CI job can open a window or touch a MIDI device.** Artifact correctness
   is proven by compile, test, and the embedded-data requirement; real launch
   behavior is a manual acceptance step.
3. **Unsigned artifacts are friction.** Gatekeeper blocks a double-clicked
   `.app` until right-click open, SmartScreen warns on the `.exe`. Documented,
   not fixed.
4. **macOS universal build.** The runner is arm64, so the x86_64 slice is a
   cross-compile. `rustup target add x86_64-apple-darwin` plus `lipo` is the
   standard path, but it is the one piece of the release workflow that cannot be
   rehearsed locally.
5. **Perf gate coverage narrows** from "CI" to "the Linux CI job". Accepted;
   documented in `docs/build.md`.

## Test plan

- Resolver: unit tests for the XDG rules (absolute, relative, missing), the
  `%APPDATA%` rule, and the fallback chain. The Windows branch runs on the
  Windows job.
- Geometry: tests assert the embedded data parses and matches the tracked files,
  and that no code path consults `CARGO_MANIFEST_DIR`.
- MIDI: `parse_sendmidi_list` (device lines, blanks, duplicates), probe
  preference (`amidi` wins when both exist), and a Windows-only
  `command_present` test with an `.exe` fixture.
- Picker: `drive_roots` returns an empty vector on Linux and macOS; a
  Windows-gated test asserts probing and current-drive exclusion.
- Existing suite must stay green on Linux with no behavioral change except the
  embedded geometry.
- Manual acceptance after task 9: unpack the Windows zip and the macOS zip on
  real machines, launch without the repo, check the module UI faceplates and a
  MIDI send, and record the result in the change's task notes.
