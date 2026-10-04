# Proposal: Fader + LED Contrast

## Summary

Give Module UI faceplate faders and LED fields the contrast they need to read as hardware: exempt the fader strip and LED dot from pause-dimming, draw a slot outline plus value marker on fader tracks, and render LEDs as ring + core with a brightness floor. Reference: `Bildschirmfoto .png` (bottom half) vs the washed Module UI cells (top).

## Problem

Three stacked causes wash the cells out (`paint_fader` / `paint_cell`, `src/gui/physical.rs`):

1. `dim()` (`gamma_multiply(0.55)`) hits **every** state indicator while processing is paused — the running-state contrast never shows in exactly the screenshots used for review.
2. The fader is only a filled strip (`fader_led_bar`) over `muted`, 2–10 px wide, with no slot outline and no value marker — at value 0 nothing reads as hardware.
3. The LED is a bare 4–10 px filled square in the cell corner: a white-only LED at value 0 is black-on-black, and lit ones have no ring, so they don't read as *fields*.

## Solution

Four levers, one change (lever 5 deferred):

1. **Pause-dim exemption** — fader strip + LED dot render full-bright while paused (status bar already reports the pause).
2. **Fader slot outline** — 1 px brighter border around the whole track, so the fader reads at value 0.
3. **Value marker** — bright 1–2 px line at fill height, reads like the reference's white cap.
4. **LED ring + brightness floor** — outline ring + filled core, ~25 % floor so an off-LED still shows its field.

Deferred: luminance floor for patch RGB colors (touches the deliberate RGB carve-out; revisit only if the comparison still disappoints).

## Non-Goals

- No new keybindings, no config keys, no toggle — fixed presentation.
- No terminal-palette color work (monochrome by design).
- No patch-RGB normalization (lever 5).

## Affected Components

- `src/gui/physical.rs` — `paint_fader`, LED block in `paint_cell`
- `src/theme.rs` — slot-outline / ring tokens across classic/terminal/mono + per-palette tests
