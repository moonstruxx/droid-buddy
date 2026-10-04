# droid_tui user manual

This is the user manual for `droid_tui`, a native desktop app for
loading, inspecting, and interacting with DROID patch files (`.ini`).
For the project front door (motivation, disclaimer, screenshots,
feature overview, build & run), see the repo-root
[`README.md`](../README.md).

## Pages

- [Getting started](getting-started.md) — install, open a patch, first
  five minutes.
- [Views](views.md) — module UI, source viewer, signal-flow graph,
  optimizer, picker, and overlays.
- [Keys](keys.md) — the full key reference, per view.
- [Labels](labels.md) — per-patch labels, shift layers, preamble labels,
  guessed labels.
- [Uploading](uploading.md) — sending the patch to DROID hardware over
  MIDI SysEx.
- [Configuration](config.md) — `config.toml` reference.
- [Contributing](contributing.md) — developer onboarding: setup, checks,
  docs rule, OpenSpec + beads workflow.

## Screenshots

Screenshots live under [`assets/`](assets/) and are rendered
deterministically by the headless paint harness (see the `readme-docs`
change) from fixed fixtures, so what you see here is what the app
draws. If an `assets/<name>.png` file is missing, its harness run has
not landed yet — the text still describes current behavior.

## Conventions

- Keys are written literally: `g g` means press `g`, then `g` within
  one second (the `g` prefix arms for 1 s).
- `Big`/`Small` panes refer to the
  [class-based pane layout](views.md#the-pane-layout): `Tab` cycles
  focus, `r` rotates the focused pane through its class carousel,
  `z` maximizes, `Esc` closes the focused view.
- Press `?` anywhere for the in-app help modal for the focused view.
