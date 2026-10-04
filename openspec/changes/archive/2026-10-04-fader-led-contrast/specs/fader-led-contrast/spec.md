# Spec: Fader + LED Contrast

## Capability

**ID:** `fader-led-contrast`
**Title:** Fader slot outline + value marker, LED ring + brightness floor, pause-dim exemption
**Status:** Proposed

## Requirements

### REQ-1: State indicators stay bright while paused

The fader lit strip and the LED indicator MUST render at full brightness while processing is paused; all other cell chrome (glyphs, labels, unlit track) keeps the existing pause-dim.

### REQ-2: Fader slot outline

Every fader cell MUST draw a 1 px outline around the full track rect in a dedicated tone brighter than the unlit track, so the slot reads as hardware at value 0.

### REQ-3: Fader value marker

Every fader cell MUST draw a bright 1–2 px line across the track at the fill height, marking the current value like a physical cap.

### REQ-4: LED ring with brightness floor

Every LED indicator MUST render as an outline ring plus filled core, with a brightness floor (~25 %) so an off-LED still shows its field. Patch-supplied RGB colors keep their hue; the floor applies to the rendered intensity.

## Data Model

New theme tokens (all three palettes): slot-outline tone, value-marker tone (may alias the strip tone), LED ring tone. Token choice (reuse vs new) is decided in `design.md`.

## Acceptance Criteria

1. Fader at value 0 shows an outlined empty slot; at value > 0 the strip, marker, and outline are all visible.
2. LED at value 0 shows a dim ringed field, not black-on-black.
3. Paused window: strip + LED full-bright, everything else dimmed (status bar still reports pause).
4. All three palettes render the new chrome distinctly; per-palette token tests pass.
