# Proposal

## Why

The Panels pane does not look like the DROID hardware: every LED renders as its own cell, elements are not at their faceplate positions, element types are wrong (M4 faders drawn as knobs, S10 rotaries drawn as toggles), and the master appears as a "CV I/O" list. The physical rack view already renders from `controller_geometry.json` at faceplate positions, so it should become the single module UI and reach roughly 90 % fidelity with the reference faceplates — every real interactive element, no surface artwork.

## What Changes

- **BREAKING** Retire the Panels view. The physical rack view becomes the module UI: it opens at startup in the left big pane, and the big-class carousel becomes graph ↔ physical.
- Fold each LED into the element it belongs to, per device; no LED renders as a standalone cell. B32/P2B8/P4B2 light the button face (B32 white-only), M4 lights the RGB touch plate below each fader, E4 draws a segmented ring around each encoder driven by the LED registers (32 per encoder), P8S8 lights the slider track, master/G8/X7 light the LED beside each jack. An explicit `led`/`ledN` pairing in the patch overrides the positional default.
- LEDs carry an RGB colour set by the patch through the colour registers; each device declares whether its LEDs are RGB or white-only.
- The master (and master18) renders as a real faceplate in its rack position — jacks, LEDs, SD button — replacing the CV I/O pseudo-panel.
- Every hardware element is drawn, including elements the patch does not use (dimmed, unlabelled).
- Every used element shows its label, resolved for the active shift layer; long labels are ellipsized and the full label appears on hover and in the status bar.
- The module UI opens fitted to the pane width; pan and zoom remain available.

## Capabilities

### New Capabilities
- `module-ui`: the physical module UI — element fidelity, LED folding per device, RGB LED colour, register-driven E4 rings, the master faceplate, unused-element rendering, shift-aware labels, fit-width default.

### Modified Capabilities
- `pane-class-layout`: the Big class no longer contains a separate module UI (panels); startup opens the physical module UI; the physical/panels mutual-exclusion requirement is removed.
- `physical-scale-model`: the compact-cell contract no longer renders LEDs as co-located cells; the default zoom fits the pane width.
- `controller-panels`: removed (superseded by `module-ui`).
- `module-scaling`: removed (zoom is owned by `physical-scale-model`).

## Impact

- Code: `src/gui/panels.rs` deleted; `src/gui/physical.rs`, `src/physical.rs`, `src/patch.rs` (LED colour + positional binding), `src/panes.rs`, `src/app.rs`, `src/handler.rs`, `src/help.rs`, `src/main.rs`, `controller_geometry.json`.
- Tests: Panels tests ported to the physical view; new regression tests per controller.
- Docs: ARCHITECTURE.md and DESIGN.md regenerate after archive (ADR 10/27/33 superseded).
