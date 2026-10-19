# Building from source

`droid_tui` is a single Rust crate that builds from the same source on
Linux, Windows and macOS. Nearly all platform-specific behavior lives in
one data-directory resolver, so the build steps are the same everywhere
and only the prerequisites differ.

This page covers a local build, the checks CI runs, and how a release is
cut. If you only want to run the app, the prebuilt downloads and their
first-launch prompts are on [Platforms](platforms.md). For the app
itself, start with [Getting started](getting-started.md).

## Prerequisites

Rust stable through [rustup](https://rustup.rs) on every platform, plus
the native toolchain the windowing and GPU crates link against.

| | Linux | Windows | macOS |
|---|---|---|---|
| Rust | stable | stable | stable |
| C toolchain | `build-essential` | MSVC build tools | Xcode command line tools |
| Build helper | `pkg-config` | — | — |
| ALSA headers, if asked for | `libasound2-dev` | — | — |
| Extra targets | — | — | both Apple targets for a universal build |
| MIDI, runtime only | `alsa-utils`, or `sendmidi` | `sendmidi` | `sendmidi` |

**Linux (Debian/Ubuntu).**

```sh
sudo apt install build-essential pkg-config
```

`build-essential` supplies the C compiler and `make`; `pkg-config` is
how the build scripts find system libraries. Fedora's equivalent is the
`Development Tools` group plus `pkgconf`, Arch's is `base-devel`.

ALSA development headers — `libasound2-dev` on Debian/Ubuntu,
`alsa-lib-devel` on Fedora, `alsa-lib` on Arch — are needed only when a
dependency's build script asks for `alsa.pc`. The app itself does not
link a MIDI library: it shells out to `amidi` or `sendmidi`, so a plain
build on a desktop install does not need them. `alsa-utils`, which
contains `amidi`, is separate again and only needed to run an upload;
see [Platforms](platforms.md#midi-upload-prerequisites).

**Windows.** Run the Visual Studio Build Tools installer and select
**Desktop development with C++**, which brings the MSVC compiler and the
Windows SDK. Rust's default host `x86_64-pc-windows-msvc` uses that
toolchain; no `link.exe` on `PATH` is the failure mode when it is
missing. A shell restart after installing is usually required.

**macOS.** One command installs clang and the tools the packaging step
needs:

```sh
xcode-select --install
```

That provides `lipo` and `ditto` as well. A universal build also needs
both Apple targets:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

### The submodule is mandatory

```sh
git clone <repo> && cd droid_tui
git submodule update --init
cargo build --release
```

`src/schema.rs` embeds
`ext/droid-lsp/droid-lsp/src/circuits.json` with `include_str!`, and
that file belongs to the `ext/droid-lsp` submodule. Without it the build
stops at compile time with a missing-file error naming `circuits.json`;
there is no runtime fallback. The geometry files
(`rack_geometry.json`, `controller_geometry.json`) are compiled in the
same way from files tracked in this repository — editing them needs a
rebuild, not a reinstall.

Run it with `./target/release/droid_tui [patch.ini]`, or
`droid_tui.exe [patch.ini]` on Windows.

## The four gates

Every change must pass these, and CI runs the same four. All four must
exit 0.

```sh
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

| Gate | Catches |
|---|---|
| `cargo fmt --check` | formatting drift; fix with plain `cargo fmt` |
| clippy with `-D warnings` | lints over every target and feature; warnings are errors |
| `cargo test --locked` | unit tests, the `src/regression.rs` stories, and the Linux perf gate |
| `cargo build --release --locked` | release-profile breakage the test profile does not show |

`--locked` turns a stale `Cargo.lock` into a failure instead of a silent
re-resolve, so a dependency change has to be committed with its lock
file. The short list in [Contributing](contributing.md) leaves it off
`cargo test`; both forms pass locally, and CI uses the locked one.

The release performance gate is a fifth command, Linux only:

```sh
cargo test --release --test perf_gate --locked
```

[The performance gate](#the-performance-gate) explains the scoping.

## What CI runs

`.github/workflows/ci.yml` runs on every push and pull request to
`master` or `main`. One job per platform:

| Job | Runner | Timeout | Runs |
|---|---|---|---|
| `ubuntu-latest` | `ubuntu-latest` | 60 min | fmt, clippy, tests, release build, perf gate |
| `windows-latest` | `windows-latest` | 45 min | tests, release build |
| `macos-latest` | `macos-latest` | 45 min | tests, release build |

The matrix sets `fail-fast: false`, so a Windows or macOS failure does
not cancel the other two jobs — one run reports all three verdicts.

Every job checks out with `submodules: recursive`, for the `circuits.json`
reason above, and installs `dtolnay/rust-toolchain@stable` with the
`clippy` and `rustfmt` components. `Swatinem/rust-cache@v2` caches cargo
between runs. The test and release-build steps are the same commands as
the local gates.

Formatting, clippy and the perf gate carry `if: runner.os == 'Linux'`,
so only the Linux job runs them:

- fmt and clippy are deterministic across targets. Running them three
  times triples the time to a verdict and adds no signal.
- The perf gate's budgets are Linux release timings. A shared Windows or
  macOS runner has a different CPU, and a flaky gate is worse than none.

Tests and the release build do run on all three, which is what proves
the crate compiles and its platform-neutral tests pass off Linux. CI
never opens a window and never touches a MIDI device; [What CI
proves](platforms.md#what-ci-proves) lists what a green run therefore
does not cover.

## Cutting a release

The version comes from the tag, not from `Cargo.toml`. Tag `v0.1.0`
gives version `0.1.0`, which names both assets and fills the macOS
bundle version. Use a plain `vX.Y.Z` tag: a prerelease suffix is passed
through into `Info.plist` verbatim, where Apple expects dotted integers.

```sh
git tag v0.1.0
git push origin v0.1.0
```

Pushing a tag matching `v*` starts `.github/workflows/release.yml`. The
same workflow can be started by hand with **Run workflow**, which asks
for the name of an existing tag — the way to re-run a failed release
without retagging. Runs are serialised per tag (`concurrency`, without
cancelling), so two runs cannot race on the same assets.

| Job | Runner | Produces |
|---|---|---|
| `Windows x86_64` | `windows-latest` | `droid_tui-<version>-x86_64-pc-windows-msvc.zip` |
| `macOS universal` | `macos-latest` | `droid_tui-<version>-macos-universal.zip` |
| `Publish release` | `ubuntu-latest` | the GitHub release, once both are done |

**Windows.** `windows-latest` is `x86_64-pc-windows-msvc`, so the
default target is the release target: `cargo build --release --locked`
yields `target/release/droid_tui.exe`, which is zipped as it is. The zip
has the executable at its root — no installer, no enclosing directory.

**macOS.** Both slices are built on the arm64 runner
(`aarch64-apple-darwin` natively, `x86_64-apple-darwin` as a
cross-build), then `scripts/package-macos.sh --version <version>` joins
them with `lipo` and zips the bundle with `ditto`. The zip unpacks to:

```
droid_tui.app/
└── Contents/
    ├── Info.plist
    └── MacOS/
        └── droid_tui
```

`Info.plist` carries bundle identifier `com.moonstruxx.droid-tui` and
puts the tag version in `CFBundleShortVersionString` and
`CFBundleVersion`. The script checks that the joined binary really holds
both `arm64` and `x86_64` slices before it finishes.

**Publishing.** The publish job downloads both artifacts and refuses to
continue unless both expected filenames are present. If the tag already
has a release — a draft from an earlier run, or one a maintainer already
published — the assets are uploaded with `--clobber`. Otherwise it
creates a **draft** release with generated notes. Publishing that draft
is a human step on the releases page.

Both artifacts are unsigned and not notarised. Signing needs an Apple
developer account and a Windows code-signing certificate, and this
project has neither. Expect Gatekeeper on macOS and SmartScreen on
Windows at first launch; [Unsigned builds](platforms.md#unsigned-builds)
has the two-step bypasses for each.

### Packaging a universal macOS build locally

The release script needs only `lipo` and `ditto`, both from the Xcode
command line tools. Build both slices first, then run it. It prints the
zip path on stdout and progress on stderr, so it can be captured:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin
./scripts/package-macos.sh --version 0.1.0
```

Output lands under `target/`: the staging bundle at
`target/macos-universal/droid_tui.app`, the zip at
`target/droid_tui-0.1.0-macos-universal.zip`. Keeping the staging
directory there leaves the repository root clean, so
`rm -rf target/macos-universal` is always safe. `--help` prints usage.

For a local Windows zip, mirror the workflow's packaging step:

```powershell
cargo build --release --locked
Compress-Archive -Path target/release/droid_tui.exe `
  -DestinationPath droid_tui-0.1.0-x86_64-pc-windows-msvc.zip -Force
```

## The performance gate

`tests/perf_gate.rs` runs pure phases over the scale anchor under named
wall-clock budgets, so a regression fails with the phase name and its
elapsed time instead of hanging the suite. The phases are
`phase_parse_render`, `phase_graph_solve`,
`phase_force_solve_and_separation`, `phase_scene_widths` and
`phase_optimizer`.

The budgets are release timings calibrated on x86_64 Linux, the
`ubuntu-latest` anchor. They are not portable — a different CPU reports
different numbers — so the whole integration test starts with

```rust
#![cfg(target_os = "linux")]
```

On Windows and macOS that attribute removes the crate, so `cargo test`
builds an empty test binary and the phases do not run at all; they are
absent, not skipped-and-failing. The gate runs in release on the Linux
CI job, where a failure fails the workflow; the other two jobs cannot
fail on it. The price is that a platform-specific regression in these
phases would surface at runtime instead of in the Windows or macOS job.

To dig into a failing phase, profile it directly:

```sh
./scripts/profile-gate.sh phase_parse_render
```

The script runs the named phase under `perf record` in release and
writes `.opencode/.tmp/profiling/<phase>.perf.data`. When
`stackcollapse-perf.pl` and `flamegraph.pl` are both on `PATH` it also
renders `<phase>.svg`; otherwise it keeps the raw profile and still
exits 0.

## Common failures

| Symptom | Cause |
|---|---|
| compile error naming `circuits.json` | submodule not initialised — `git submodule update --init` |
| `link.exe` not found on Windows | MSVC build tools missing, or the shell predates the install |
| clippy warnings in files you did not touch | CI lints `--all-features`; a dependency feature you enabled locally is covered too |
| perf gate fails only locally | run it in release, on Linux; the budgets are calibrated there |

## See also

- [Platforms](platforms.md) — support matrix, data directories, MIDI
  prerequisites, unsigned builds, known limits.
- [Contributing](contributing.md) — setup, the checks short list, the
  OpenSpec and beads workflow.
- [Getting started](getting-started.md) — run the app and open a patch.
- [Uploading](uploading.md) — the SysEx send and its confirm modal.
