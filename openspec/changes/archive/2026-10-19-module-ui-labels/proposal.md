# Proposal

## Why

The module UI's element labels exist but are never visible. Labels resolve
correctly today — per-shift stored label, then layer 1, then the patch preamble,
then the derived token — and `e` edits them, but the view draws at 1.5 pt per
millimetre, roughly 40 % of true size, so no faceplate cell can hold text: a 9 mm
button is 13.5 pt against a fixed 10 pt label font, and the cell path subtracts a
state glyph and a space before budgeting characters, so the budget is zero. Pots
get one character, P8S8 faders (4.2 mm) and master LEDs and jacks (3 mm) never
fit at any preset. The word "1:1" in `physical-scale-model` is nominal: the scale
constant is a terminal-era 0.15 cells/mm, and the isotropic fix that made a round
button round halved every cell's height while the label font stayed fixed, so
button faces lost their labels outright.

Three behaviours the specs already require are unimplemented, and each one is why
a user cannot get at the labels:

- `module-ui` "Fit-width default" — the module UI never fits itself to its pane.
  The window opens at the configured zoom anchored at offset `(0, 0)`, so a wide
  rack starts cropped.
- `module-ui` "Shift-aware element labels" — the full label on hover and in the
  status bar does not exist: there is no tooltip anywhere in the app, and hovering
  an element writes nothing to `status_message`.
- `physical-scale-model` "1:1 main view" — overflow panning is dead in the window:
  `physical_viewport` and `physical_rack_size` are written only by tests, so the
  rack never reports an overflow and the arrow keys never pan; and the paint
  routine's own frame report (wheel zoom, middle-drag pan, skeleton toggle) is
  discarded by the pane dispatch, so the wheel over the rack does nothing at all.

Labels are therefore not a feature to switch on but a presentation to unblock.

## What Changes

- **True 1:1 baseline.** One scale constant replaces the two-factor divide:
  100 % zoom means one millimetre is 3.7795 pt (the 96-dpi reference), so a 9 mm
  button cell is 34 pt, a 12 mm pot 45 pt, an 18 mm M4 button 68 pt. The zoom
  range widens to [0.25, 4.0]; the `+`/`-` preset ladder is unchanged.
- **Fit-on-open and `f`.** The view fits the rack to its pane width and height and
  centres it — on open, on patch load, on a rack change, and on pane resize — until
  the user zooms or pans, after which `f` returns to the fit. A single-row rack up
  to ~165 mm now opens at 100–107 %; a 173 mm rack at ~98 %; a 400 mm rack at ~42 %.
- **Working viewport input.** The paint routine's frame report is consumed: the
  wheel over an element keeps adjusting its value, the wheel off an element and
  Ctrl+wheel anywhere zoom about the cursor, and a middle-drag pans. Overflow
  detection uses the live pane rect instead of the terminal-era viewport fields.
- **Readable labels.** A label presentation with three modes — `off`, `auto`
  (default), `on` — cycled with `a` and persisted as `[physical] labels`. In `auto`
  the view annotates exactly the elements whose in-cell label is clipped or hidden
  at the current zoom, placing them outside the rack with leader lines at the
  performance view's 14 pt size, never duplicating a label the cell already shows
  and never labelling an unaddressed element. When the margin would exceed 30 % of
  the pane, labels drop in a fixed priority order (selected and focused last to
  drop, LEDs and jacks first) and the status counts what is shown and what is
  hover-only. `on` annotates every labelled element.
- **Hover reveal.** Hovering any element shows its full label and state as a
  callout anchored to the cell, in every mode — the spec'd reveal that makes small
  families reachable without changing the scale.
- **Presentation feedback.** The module-UI pane's title carries the view state
  (`Module UI · 98% fit · labels auto · skeleton off`), and help documents `f`, `a`,
  and the wheel and drag gestures.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `module-ui`: element labels become readable — a presentation mode with a
  clipping-driven annotation rule and a drop budget, a hover reveal, and fit-on-open
  for the pane; the interaction contract gains wheel/Ctrl+wheel zoom and
  middle-drag pan alongside the value-adjust wheel.
- `physical-scale-model`: 100 % zoom becomes true 1:1 with one constant and one
  mapping, the supported zoom range widens, the view fits itself to the pane, and
  the compact-cell contract states what happens to a label the cell cannot hold.
- `keybinding`: the module UI gains `f` (fit and centre) and `a` (cycle the label
  presentation), the pan keys act against the live pane rect, and the help content
  requirement covers the new rows and gestures.

## Impact

- Touched: `src/physical.rs` (scale constant, mapping), `src/gui/physical.rs`
  (label budgets, annotation layer, hover callout, frame report), `src/app.rs`
  (fit helper, zoom range, overflow against the live pane), `src/config.rs`
  (`[physical] labels`, zoom clamp, one-time zoom baseline reset), `src/handler.rs`
  (frame routing promoted out of tests, `f`, `a`, pan gating), `src/gui/mod.rs`
  (pane title suffix, frame consumption, pane-rect publication), `src/main.rs`
  (startup seed), `src/help.rs` (Module UI table), `src/regression.rs`,
  `tests/perf_gate.rs`, `fixtures/`, `docs/assets/`.
- Visible behaviour: the module UI opens fitted and roughly twice its previous size,
  so faceplates read at true proportion and cells stop being half empty; labels
  appear on the rack where they fit and beside it where they do not; the wheel zooms
  and the middle button pans; a status line names the view state. Composed button
  faces, fader tracks, switch glyphs, the skeleton presentation, LED folding, the
  performance view's compact rack, and the coincidence contract between skeleton
  rects, drawn cells, and published hit rects are unchanged.
- Migration: existing `[physical] zoom` values were saved against the old baseline,
  so they are dropped once — the view opens fitted instead — and a baseline marker
  written on the next config save. No `.ini` mutation, no new dependency, no crate
  feature, no persistence beyond the existing `config.toml` and `labels.toml`.
- Review point: the 1:1 rebaseline changes the size and fit of every physical-view
  artifact and every test that encodes a point size, so the re-rendered shots and the
  recalibrated assertions are the evidence to review first.

## Non-goals

- No change to the composed button face, the fader track, switch glyphs, the
  skeleton presentation, LED-fold rules, unaddressed-cell dimming, or the
  performance view's compact mapping.
- No abbreviation scheme or short-form label synthesis: labels are user strings
  (store or preamble) and inventing a canonical short form is a data-model change,
  not a presentation one.
- No scrollbars, no zoom-to-cursor on a single value-adjusting element, no
  per-element font scaling.
- No change to click semantics (a click focuses, `Enter`/`Space` toggles,
  `Ctrl+Enter` toggles the folded LED), to `controller_geometry.json`, or to
  `tools/acquire_geometry.py`.
- No rendered text below 8 pt: a label that cannot be drawn legibly is annotated or
  revealed on hover instead of shrunk.
- No hand-edits to ARCHITECTURE.md or DESIGN.md; the lockstep docs regenerate at
  archive.
