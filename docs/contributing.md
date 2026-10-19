# Contributing

Deep onboarding for contributors. The repo-root `README.md` carries
the short version; this page is the complete one.

## Setup

```sh
git clone <repo> && cd droid_tui
git submodule update --init   # ext/droid-lsp: the circuit-schema source of truth
cargo build
```

`ext/droid-lsp` is a vendored git submodule — without it the build
has no schema to embed. The binary runs with `./target/release/droid_tui
[patch.ini]` (`droid_tui.exe` on Windows), falling back to an embedded
demo patch. Native prerequisites per OS — MSVC build tools, Xcode
command line tools, `build-essential` and `pkg-config` — are on
[Building from source](build.md#prerequisites).

## The four gates

All four must exit 0 before reporting completion. These are the exact
commands CI runs:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

`--locked` makes a stale `Cargo.lock` a failure instead of a silent
re-resolve, so commit a dependency change together with its lock file.

Tests are co-located `#[cfg(test)]` unit tests plus
`src/regression.rs` (cross-layer stories) and `tests/perf_gate.rs`
(budgeted scale-anchor phases). Rendering is verified by headless
egui shape/label assertions — no GPU, no live window.

### What runs where

CI is a three-platform matrix: `ubuntu-latest`, `windows-latest` and
`macos-latest` each run `cargo test --locked` and
`cargo build --release --locked`, with `fail-fast: false` so one run
reports all three verdicts.

The rest is Linux-only, behind `if: runner.os == 'Linux'`:
`cargo fmt --check` and clippy are deterministic across targets, and
the release performance gate's budgets are Linux timings.

```sh
cargo test --release --test perf_gate --locked   # Linux only
```

On Windows and macOS, `tests/perf_gate.rs` starts with
`#![cfg(target_os = "linux")]`, so the phases are absent rather than
skipped-and-failing. A change to those phases is therefore proven on
Linux only; see [The performance gate](build.md#the-performance-gate).

## Derived docs rule

`ARCHITECTURE.md` and `DESIGN.md` are **generated** artifacts — never
hand-edit them. Regenerate via the `/make-*` commands. During OpenSpec
archive, the docs hand-edit step is skipped; flag stale wording in
the report for regeneration instead of patching inline.

## Change workflow (OpenSpec + beads)

- Propose under `openspec/changes/`, implement, archive to
  `openspec/changes/archive/`, sync capability specs to
  `openspec/specs/`. No behavior change without a spec delta.
- All task tracking goes through `bd` (beads): `bd ready` for
  available work, `bd update <id> --claim`, `bd close <id> --reason`.
  No markdown TODO lists.
- Docs-only changes (like this manual) touch `docs/**` only and carry
  no spec deltas.

Build and platform details live in the manual rather than here:
[Building from source](build.md) for the gates, CI jobs and releases,
[Platforms](platforms.md) for support, data directories and MIDI
prerequisites.

## Where code goes

Single crate, one module per file (`src/<name>.rs` + `pub mod` in
`src/lib.rs`): `patch.rs` (parser), `app.rs` (state), `handler.rs`
(input), `graph.rs` + `layout.rs` (signal flow), `schema.rs` +
`validation.rs` (schema + checks), `gui/` (egui surfaces). New pure
modules stay UI-free; new surfaces dispatch from `src/gui/mod.rs`.
See `STRUCTURE.md` for the full map.
