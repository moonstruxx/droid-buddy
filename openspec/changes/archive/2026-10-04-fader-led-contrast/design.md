# Design: Fader + LED Contrast

## Overview

All work lands in the existing paint path (`paint_fader` + the LED block of `paint_cell`, `src/gui/physical.rs`) plus theme tokens (`src/theme.rs`). No new surfaces, keys, or config.

## Decisions

### Pause-dim exemption (lever 1)

Gate the `dim(..., paused)` calls for the lit strip and the LED dot on "not paused OR exempt": pass `paused = false` for exactly those two fills. Everything else keeps `paused`. One-line change per fill site; no signature changes.

### Slot outline + value marker (levers 2 + 3)

- Outline: `rect_stroke` around the track rect, 1 px, zoom-independent (chrome, not world geometry).
- Marker: filled rect across the track width at `track.max.y - fill_h`, 1–2 px tall (2 px when track ≥ 6 px wide, else 1 px), drawn over strip/track boundary.
- Token decision (open, recommend new): a new `fader_track` tone risks palette sprawl; reusing `muted` brighter is impossible (single token). **Recommended:** new `fader_slot` token (classic: mid gray brighter than `muted`; terminal: Reset; mono: distinct gray), marker aliases `fader_led_bar`.

### LED ring + floor (lever 4)

- Ring: `rect_stroke` around the LED rect in the ring tone; core: inset filled rect with the existing color logic (patch RGB or white × value).
- Floor: clamp rendered intensity to [0.25, 1.0] for the core; ring always full tone.
- Token decision: **recommended** new `led_ring` token (classic: White-ish; terminal: Reset; mono: White) — reusing `led` (Red) would tint every ring red including white-only fields.

### Token matrix + tests

Every new token needs classic/terminal/mono values plus `*_resolves_per_palette` assertions (existing test pattern in `src/theme.rs`) and headless shape tests (stroke rect present, marker at fill height, ring rect present) in `src/gui/physical.rs`.

## Risks

- More chrome per cell costs shapes; fader/LED cells are a minority of cells — negligible.
- Brightness floor slightly changes `led` semantics: documented in the token comment.
