# Design

## Context

`controller_geometry.json` is generated, not hand-written:
`tools/acquire_geometry.py` (stdlib only, deterministic, self-verifying via
`--check`) emits one entry per controller — module width/height plus
`element_cells` per family letter — and the app consumes only `element_cells`
(`src/physical.rs::ControllerGeometry`, `ElementCellEntry`: `col`, `row`,
`x_mm`, `y_mm`, `w_mm`, `h_mm`, `label`; the `grids`/`token_counts`/`notes`
blocks are documentation no code reads).

Two facts make this a data-and-test change rather than a code change:

- Tokens match cells by `(family key, element number decoded from the label)` —
  `physical.rs::cell_for` — so a family's arrangement (one column versus two
  columns) is invisible to the lookup.
- The renderer derives every glyph from `HwComponent.kind`
  (`gui/physical.rs::cell_visuals`), never from a cell's `col`/`row`.

Where the arrangements came from: every affected entry's `notes` says
`"Assumed: pitches/margins"`. The generator guessed. A second, coarser
geometry source already disagrees — `rack_geometry.json` declares
`"e4": { "kind": "stack", "count": 4, "pitch_y": 2 }`, a vertical stack, while
`controller_geometry.json` puts the four encoders in four columns.

## Goals / Non-Goals

**Goals:**

- Every affected faceplate puts its elements where the hardware does.
- The numbers are measured and reproducible, with the method recorded so the
  next re-measurement is a re-run, not a rebuild of trust.
- The two geometry files agree wherever they describe the same grid, enforced
  by a test rather than by review.
- The change is provable: the regression assertions exist before the data, and
  a rendered artifact shows all six corrected faceplates.

**Non-Goals:**

- Any controller this change does not touch (B32, P8S8, M4, G8, X7, master,
  R2M/R2C) — its guessed numbers stay as they are.
- The unimplemented S10 rotary-versus-toggle glyph (see proposal.md
  Non-goals).
- Renderer changes: no code path needs to know the arrangement.

## Measured geometry

Method: the manual's faceplate images are drawn to scale, and each panel pins
the px/mm factor twice independently — panel width against its HP width, panel
height against 128.5 mm (3 HE):

| Panel class | Width | Height | Factor |
|---|---|---|---|
| P2B8, P4B2, P10, S10 (5 HP) | 224 px ↔ 25.4 mm | 1132 px ↔ 128.5 mm | 8.82 px/mm |
| E4, DB8E (6 HP) | 225 px ↔ 30.48 mm | 953 px ↔ 128.5 mm | 7.38 px/mm |

Element centres come from blob centroids of the element caps, validated
against the printed label grid. Calibration check: the measured P2B8 button
`pitch_y` is 15.0 mm — identical to the model's existing 15.0 mm — so the
factor and the origin are trusted. Pot and rotary centres come from the panel
centre column and the printed `0…1` scale marks, because a cap centroid skews
by up to ~1 mm where the pointer covers one side.

All values are mm from the faceplate's top-left corner; `±0.3 mm` is the
measurement ceiling at this source resolution.

| Controller | Family | Element centres | Cell | Pitch |
|---|---|---|---|---|
| P2B8 | P | (12.7, 19.1), (12.7, 43.8) | 12×12 | y 24.7 — one column |
| P2B8 | B / L | x 7.1 / 19.7; y 68.5 / 83.5 / 98.4 / 113.4 | 9×9 / 3×3 | x 12.6 (was 9.2), y 15.0 |
| P4B2 | P | (12.7, 19.1), (12.7, 43.8), (12.7, 68.7), (12.7, 93.5) | 12×12 | y 24.8 — one column |
| P4B2 | B / L | x 7.1 / 19.7; y 113.4 | 9×9 / 3×3 | x 12.6 |
| P10 | P big | (12.7, 19.3), (12.7, 44.1) | 12×12 | y 24.8 — one column |
| P10 | P small | x 6.9 / 19.5; y 68.6 / 83.7 / 98.8 / 113.9 | 9×9 | 12.6 × 15.1 — 2 × 4 matrix |
| S10 | S rotary | (12.7, 18.5), (12.7, 43.3) | 12×12 | y 24.8 — one column |
| S10 | S small | x 6.9 / 19.5; y 68.6 / 83.7 / 98.8 / 113.9 | 8×12 | 12.6 × 15.1 |
| E4 | E, shaft B | (15.24, 24.3), (15.24, 52.1), (15.24, 79.9), (15.24, 107.7) | 7×7 / 3×3 | y 27.8 — one column |
| E4 | L rings (×4) | centred on each encoder | 1.8×1.8, 9 per side, pitch 2.76 | ring 23.9 mm |
| DB8E | B | x 7.9 / 23.1; y 41.4 / 56.6 / 71.6 / 86.8 | 9×9 | 15.2 × 15.1 |
| DB8E | E + ring | (15.24, 108.1) | 7×7 + 23.9 mm ring | below the button grid |

Fit checks against the 128.5 mm panel: E4 ring 1 top edge 12.35 mm, ring 4
bottom edge 119.65 mm, adjacent-ring gap 3.9 mm; DB8E ring top edge 96.15 mm
against a button grid ending at 91.3 mm. The shared ~24.8 mm pitch across
P2B8/P4B2/P10/S10 is one pot class on one panel grid — independent support
that the measurement is right.

## Decisions

- **Vertical stack over the two-cell row.** The hardware stacks these families;
  `rack_geometry.json` already said so for E4. The model's row was a generator
  default, not a hardware fact.
- **Measured over assumed.** The grep-able signal that this round of geometry
  is trustworthy is the provenance string: `notes` changes from
  `"Assumed: pitches/margins"` to the measured values plus the calibration
  factor, so the next reader can tell guess from measurement.
- **Cell sizes keep the existing cap-plus-margin convention.** Pot 12, encoder
  7, small pot 9, small switch 8 × 12, ring LED 1.8; the button grows 8 → 9 to
  the measured 9.1. The one size that must change hard is the ring, whose LED
  cells went from 1 mm at 9 mm overall to 1.8 mm at 23.9 mm overall.
- **The ring is the hardware's dotted square**: 9 LEDs per side at 2.76 mm
  pitch, driven per encoder by the existing label range (L1.1–L1.32 for the
  first, and so on). `ring_32` gains pitch and cell-size parameters instead of
  a second helper.
- **The DB8E encoder moves below the button grid**, with the 32-LED ring around
  it. Its display band is unaffected: the OLED placeholder derives its height
  from the module's `cells["B"]` top, which moves with the buttons.
- **Nothing in production code changes.** If an implementer finds themselves
  editing `src/physical.rs` logic rather than its tests, the arrangement
  assumption has leaked somewhere and that is a design finding, not a task.
- **TDD ordering is explicit.** Task 1.1 lands the assertions first and they are
  expected to fail; task 2.1 turns them green. The suite is red between the two
  by design — an assertion must never be relaxed to the old geometry to make it
  pass, and task 4.1 gates on 2.1.
- **One fixture, for the eye, not for the tests.** `src/physical.rs` tests
  build patches inline through the existing `patch(...)` helper; the new
  fixture exists only so the screenshot harness can show all six faceplates in
  one render.
- **Cross-file agreement is a test, not a convention.** `rack_geometry.json`
  already carries the E4 stack; `src/geometry.rs` gets the agreement test
  beside the existing B32 4 × 8 one, so the next drift fails the suite.

## Risks / Trade-offs

- [Risk] Measurement precision is ±0.3 mm at this source resolution →
  Mitigation: the factor is cross-checked on two axes and validated against an
  independently known pitch (P2B8 buttons, 15.0 mm exact); values are recorded
  rounded to 0.1 mm and the method is written down so a re-measure supersedes
  rather than re-guesses.
- [Risk] The E4 ring's 1.8 mm dots may vanish or blob at the pane's fit zoom,
  making the ring read as noise or a solid band → Mitigation: the new shot is
  the check; if a leg stops reading, widen the LED cell toward the ring pitch
  (up to 2.76 mm) — a render-legibility knob that leaves the measured ring
  extent and the register mapping alone. Do not shrink the ring back.
- [Risk] The red suite window between tasks 1.1 and 2.1 invites an agent to
  "fix" the tests by matching the current data → Mitigation: the instruction is
  explicit in task 1.1 and restated here; 4.1 depends on 2.1, so a gate cannot
  pass on relaxed assertions.
- [Risk] `grids` / `notes` are documentation no code reads, so they can drift
  from `element_cells` silently → Mitigation: `--check` verifies the declared
  grid shapes and pitch values against the emitted cells; the Rust tests pin
  the arrangement itself.
- [Risk] `p4b2` is declared by name but is not bare-synthesized, so a bare
  `[p4b2]` faceplate renders with dimmed, unlabelled elements → Accepted for
  the fixture: it doubles as the unused-element proof, and explicit entries can
  be added if a labelled render is wanted.
- [Risk] Re-blessing `docs/assets/*.png` is opaque churn in the diff →
  Mitigation: the new shot is the reviewable artifact; the re-blessed PNGs are
  eyeballed against it in task 3.2.
