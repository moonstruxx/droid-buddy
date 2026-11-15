# Design

## Context

See `proposal.md` — Why. Four facts shape the approach.

1. `paint_cell` (`src/gui/physical.rs:1324`) already dispatches on a cell's shape: `is_fader` → `paint_fader`, `cell.shape.is_some()` → `paint_composed` (a button face that keeps the cell and labels the band beneath it), an unaddressed rotary → ring plus dot, then the compact glyph/label path. A new face is a new branch in an existing dispatch, not a new paint path.
2. `CellShape` (`src/gui/physical.rs:94`) is the "this cell has a face" discriminant and `CellTextPath` (`src/physical.rs:1784`) the "which text box the label is fitted to" discriminant; `cell_label_box` is the single bridge between them, and `label_layout` is the single label authority. Both already carry `Composed`, so a face with a band is a solved shape.
3. The geometry sets the size range: 12 mm pots (P2B8, P4B2, P10 big) 45 pt, 9 mm pots (P10 small) 34 pt, E4/DB8E encoders 26 pt, master CV jacks 26 pt at 1:1; 17 / 13 / 10 pt at the 37 % rack fit of `fixtures/physical_multirow_rack.ini`.
4. `theme::Theme` already has `knob`, `cv_in` and `cv_out`; the `mono` palette maps `cv_in` and `cv_out` to the same value, which forbids colour as the direction channel.

## Goals / Non-Goals

**Goals**
- A faceplate reads as hardware at 1:1 without faceplate artwork: pots, encoders and jacks have silhouettes, not glyphs.
- The value is visible without a text row at true size, where the band has no room for one.
- Every zoom from 25 % to 400 % keeps each element identifiable and hit-testable exactly as before.

**Non-Goals**
- No new paint path, no new theme token, no node icons in the graph.
- No change to the layout model, the label ladder, the annotation budget or the fit scan — only the text box the ladder is measured against.

## Decisions

### D1 — Faces ride the existing `CellShape` / `CellTextPath` seam

`CellShape` gains `Pot`, `Encoder` and `Jack` beside `Round` and `Rect`; `CellTextPath` gains `Face` beside `Compact`, `Composed` and `Fader`. Their consumers are named in scope: `paint_pot` and `paint_jack` reached from `paint_cell`, and `label_text_box` reached from `cell_label_box`. The variant name is the element kind, so the painter reads finiteness (base line or not) off the shape instead of carrying a second flag.

*Alternatives considered.* A `face: Option<FaceKind>` field beside `shape` would duplicate the discriminant and let the two disagree; a `Face { legs: bool }` payload on `CellShape` buys one boolean and costs `Eq` plus a match arm per painter. Rejected under DRY/YAGNI.

### D2 — One face depth, `0.66 w`

Pot, encoder and jack share `FACE_HEIGHT_FRACTION = 0.66`, so one constant and one band rule (`h − 0.66 w`) serve all three, exactly as `COMPOSED_FACE_HEIGHT_FRACTION = 0.5` serves a button face. The pot spends the lower `0.11 w` of its box on the base line and centres a `0.60 w` circle in the rest; encoder and jack centre a `0.60 w` circle in the full box, which keeps the three symbols at one visual size.

At a 45 pt pot cell the band is 15.3 pt: the ladder takes the largest step that fits — one 8 pt line, or two at 6 pt when the written label is longer. A 34 pt P10 pot leaves 11.6 pt (one 7 pt line) and a 26 pt jack leaves 8.8 pt (one 6 pt line). The ladder already prefers the larger single line, so the depth does not need to shrink to protect the label.

*Alternatives considered.* A per-kind depth (`0.72` to make room for the reference icon's three legs) costs a `LabelBox` field, a second band constant and ~3 pt of label band on every pot, for a detail that is invisible at the rack fit. A shallower `0.55` face would free two label lines but shrink the symbol below the size at which the nut resolves — the one thing the face exists for.

### D3 — Level of detail on the drawn face diameter, at 18 pt and 10 pt

Full symbol at or above an 18 pt face: the nut's hex edge is then ~8.8 pt, still a hexagon at a 1 pt stroke. From 10 to 18 pt the rotary drops the nut, the shaft and the base line to a ring plus a slot — the slot survives because it is one stroke across the middle. Below a 10 pt face both symbols reduce to a ring plus a centre dot, which is the shape an unaddressed rotary already draws today, so the smallest cells keep the footprint they have.

The thresholds are on the *drawn* face, not on the cell or the zoom, so the ladder is automatically correct at 25 %, at the two fits and at 400 %, and needs no per-preset table.

### D4 — The value rides the slot, and the state row is not special-cased

The slot sweeps 270°, 0 at 7:30 and 1 at 4:30, mirroring a real pot's travel. No rule is added for the percentage row: `label_layout` already tries "label plus state row" first and yields the row when the band cannot hold both, so at 1:1 a pot shows the face and one label line, and at a large zoom the same cell shows the label plus `50%`. Exact values stay reachable through the hover reveal and the status line, which already report them.

*Alternatives considered.* Keeping the percentage by shrinking the face would trade the symbol for a number the hover already gives.

### D5 — Silhouette separates addressed from unaddressed, not brightness

An unaddressed rotary drops the washer ring (the ring is what *addressed* adds) and keeps the nut, the slot and, on a pot, the base line; an unaddressed jack keeps its full face. Both are dimmed by the existing `dim` path. This retires the ring-plus-dot rule (`DESIGN.md:224`) for rotaries only — every other kind keeps the plain dim dot.

Below an 18 pt face the reduced ladder cannot carry the difference, so pot and encoder converge, as they already do today where both render `◉`; the label still names them.

### D6 — Direction never rides colour

`mono` maps `cv_in` and `cv_out` to the same Gray, and the `terminal` palette maps them to `Reset`, so a fill colour cannot be the direction channel. The `◀`/`▶` corner marker (the existing `paint_state_marker` path) and the `CV IN`/`CV OUT` state row stay; the jack face carries identity only. No token is added, so the three palettes and their compile-time table stay untouched.

### D7 — `fader_value` becomes `value_frac`

`CellSpec::fader_value` already carries "the element's 0..1 position" and is already computed for every component in `physical_spec` (`fader_value_for`). The slot reads the same number, so the field is renamed rather than joined by a second one; `paint_fader` and its tests take the new name.

## Risks / Trade-offs

- **The labels-fit zoom and the `auto` annotation set move.** Every pot and jack cell's text box shrinks from the whole cell to `h − 0.66 w`, so the fit lands lower and dense racks (`fixtures/physical_multirow_rack.ini`, an inventoried master) annotate more labels than today. → Pinned by regression stories in task 3.2; the annotation budget and its drop order are unchanged, so the extra callouts are dropped in the existing family order rather than crowding the rack.
- **A pot loses a text line at 1:1**: one 8 pt line instead of two 7 pt lines plus `0%`. → Accepted: the value moves to the slot, and the hover reveal keeps the exact percentage.
- **A rotary angle is less precise than a number.** → The state row returns wherever the band allows it, and the status line reports the value on wheel adjust.
- **Pot and encoder look identical below an 18 pt face.** → Same as today's shared `◉`; the label and the token prefix (`Pot` / `Encoder`) carry the kind, and they differ again at every zoom above ~55 %.
- **Drawing cost per frame**: up to three shapes and two strokes per rotary cell, over a rack fit drawing hundreds of cells. → The LOD ladder collapses the smallest cells to a ring plus a dot, which is the cost the painter already pays today, and `PhysicalSpec` builds once per frame, not once per element repaint.

## Migration Plan

None — a display change behind existing types. No persisted state, no config key, no data migration. Reverting is deleting the two painters and the three shape variants.
