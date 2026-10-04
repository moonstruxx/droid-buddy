# fader-led-contrast Specification

## Purpose

Fader and LED cells on the physical 1:1 view meet contrast floors so they read as hardware: the fader slot is outlined with a value marker even at value 0, LED fields show a ring with a brightness floor even when off, and both stay bright while processing is paused.

## Requirements

### Requirement: State indicators stay bright while paused

The fader lit strip and the LED indicator MUST render at full brightness while processing is paused; all other cell chrome (glyphs, labels, unlit track) keeps the existing pause-dim.

#### Scenario: Strip and dot exempt

- **WHEN** processing is paused and a fader/LED cell is visible
- **THEN** the lit strip and LED dot render undimmed while the cell's text and unlit track render dimmed.

### Requirement: Fader slot outline

Every fader cell MUST draw an outline around the full track rect in a tone brighter than the unlit track, so the slot reads as hardware at value 0.

#### Scenario: Empty slot reads as hardware

- **WHEN** a fader is at value 0%
- **THEN** the cell shows an outlined empty slot rather than an unmarked dark area.

### Requirement: Fader value marker

Every fader cell with a nonzero value MUST draw a bright marker line across the track at the fill height, marking the current value like a physical cap.

#### Scenario: Marker tracks the value

- **WHEN** a fader is at a given value above 0%
- **THEN** a marker line renders at the corresponding fill height in the strip tone.

### Requirement: LED ring with brightness floor

Every LED indicator MUST render as an outline ring plus a filled core, with a brightness floor (about one quarter) applied to the rendered intensity, so an off-LED still shows its field. Patch-supplied RGB colors keep their hue.

#### Scenario: Off LED shows its field

- **WHEN** an LED is at value 0
- **THEN** the cell shows a ringed field with a dim core instead of black-on-black.
