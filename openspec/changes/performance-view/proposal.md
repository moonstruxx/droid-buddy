# Proposal

## Why

Module-UI cells render labels at a fixed 10pt monospace clipped to the cell
width (`clip_label`), so on a tiny 1:1 rack most labels are unreadable stubs —
full text exists only on hover + status bar. Labels are important, but there is
nowhere to put them at physical scale. DROID hardware supports no dynamic
labels at all, which makes our own label guessing (tree-inherited,
nearest-explicit-upstream) the important feature: it is the only source of
meaningful names for unlabeled circuits.

## What Changes

- A performance view for the module UI: the rack renders compact and centered
  (~1/4 of the Physical pane) while every element label renders big in the
  freed field around it, each joined to its host cell by a leader line. Labels
  never enter the rack rect.
- `p` with the Physical pane focused opens the performance view (view-scoped;
  `p` everywhere else keeps its current meaning: global processing pause,
  pin/unpin on the graph pane). `Esc` backs out.
- Side-bound placement: each label is assigned over / under / left / right by
  host position relative to the rack center and stays bound there (stable
  across frames); labels stack along their side to de-collide.
- Label resolution is store-defined → guessed → derived token, so guessed
  labels are the headline content. Element activation still animates live
  state (glyphs/values) in rack and callouts; a reset key restores all
  element states to rest.
- Help table documents the new keys.

## Capabilities

### New Capabilities

- `performance-view`: exploded-label performance view with side-bound
  callouts, leader lines, guessed-first labels, and element-state reset.

### Modified Capabilities

- `keybinding`: view-scoped `p` on the Physical pane, new reset key, `Esc`
  exit, help rows.

## Impact

- Touched code: `src/app.rs` (view state, reset-all), `src/handler.rs`
  (`p`/reset/`Esc` routing), `src/gui/physical.rs` (performance
  presentation), `src/theme.rs` (one leader-line token), new
  `src/performance.rs` (pure placement model), `src/help.rs` (rows).
- No new dependencies, no config keys, no persistence, no `.ini` mutation.

## Non-goals

- No full-band takeover (pane-local first; takeover is a later option).
- No new label sources beyond the existing store → guess → derived chain.
- No change to drag/pin/settle, processing pause, or optimizer flows.
- No ARCHITECTURE.md/DESIGN.md hand-edits (derived artifacts).

## New types (YAGNI consumer check)

- `PerformanceLayout` (per-label side/band slot + leader anchor, in new
  `src/performance.rs`): consumed by the `gui/physical.rs` performance
  painter; no other new types planned. View state is a bool on `App`.
