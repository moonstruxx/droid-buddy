# Design

## Context

See `proposal.md` — Why. The constraints that shape the approach:

- The module UI draws from one geometry source: `physical::ScreenMapping` turns
  millimetres into screen points, `gui::physical::rack_geometry` turns those into
  the module, element, fold-bar, and mount rects, and one filter feeds the drawn
  cells, the published `component_rects` hit rects, and the skeleton presentation.
  Any change to the scale is a change to all three at once (ADR 35, the
  coincidence contract).
- The mm→point scale is currently two constants multiplied: `PHYSICAL_CELLS_PER_MM`
  (0.15) in `src/physical.rs` and `PHYSICAL_CELL_SCALE` (10.0) in
  `src/gui/physical.rs`, giving 1.5 pt/mm at zoom 1.0 — about 40 % of true size.
- Label fonts are fixed in points and contract-pinned
  (`cell_label_font_is_identical_across_zoom_presets`,
  `regression_anchor_module_ui_label_fonts_constant_across_zoom`): 10 pt monospace
  in cells (6 pt advance), 9 pt proportional state text, 14 pt monospace callouts in
  the performance view, 11 pt pane chrome.
- The paint routine already computes a complete input report — `PhysicalFrame`
  carries wheel zoom with a cursor anchor, middle-drag pan, a skeleton toggle, and
  the same scene the terminal path used — and the pane dispatch throws it away
  (`src/gui/mod.rs`, `let _ = physical::paint_physical(..)`). Its handler,
  `handler::handle_physical_frame`, exists but is `#[cfg(test)]`.
- `physical_viewport` and `physical_rack_size` are written only by tests, so
  `App::physical_overflow` never reports an overflow in the running app.
- Label text resolves through `Patch::display_label` (store layer → layer 1 →
  preamble → derived) and is already shift-aware and editable; only its
  presentation is missing. `App.labels` carries the `[labels]` policy from startup.
- `src/performance.rs` already implements deterministic, overlap-free side-bound
  label placement with leader anchors and is proven in the performance view.

## Goals / Non-Goals

**Goals:**

- Make every addressed element's label reachable and readable at every zoom,
  without shrinking text below a legible size and without inventing label data.
- Make the 1:1 claim true and make the view open in a state the user can read.
- Make the pointer and key gestures on the module UI do what the specs promise:
  fit, zoom about the pointer, pan, adjust a value, reveal a label.
- Keep one geometry source: decorations must not be able to desynchronize the
  drawn cells, the published hit rects, and the skeleton.

**Non-Goals:**

- No label abbreviation or synthesis, no editable-in-place annotation, no
  per-element fonts, no scrollbars, no change to the performance view's mapping.
- No change to the graph pane's keys (`f`, `a`, `c`, `Shift+c` stay graph-scoped)
  and no change to `controller_geometry.json` or the geometry tooling.
- No re-derivation of the annotated data model: annotations read the same
  `CellSpec.label` the cell painter reads.

## Decisions

### D1 — One true-1:1 scale constant

Replace `PHYSICAL_CELLS_PER_MM` (0.15) and `PHYSICAL_CELL_SCALE` (10.0) with a
single `PHYSICAL_POINTS_PER_MM: f64 = 3.779527559` (the 96-dpi reference), and make
`ScreenMapping` carry points per millimetre in a field named accordingly.

*Why*: the two-factor divide is a terminal-era unit (`cells`, at 0.3 cells/mm
vertically) plus an invisible ×10 points-per-cell. One number with an honest unit
makes 100 % zoom mean true size and removes the class of bug where a reader assumes
the wrong factor. *Alternatives*: (a) keep both constants and only change 0.15 to
0.377953 (the memo's minimal edit) — smaller diff, keeps a misleading unit and a
hidden factor; (b) keep cells/mm and multiply by a points-per-cell DPI factor at the
call sites — same confusion, spread out. Rejected both: the rename is mechanical
(~14 sites plus tests) and this change is the only occasion to do it.

*Consequence*: `[physical] zoom` values and every test asserting a point size are
rebaselined by ×2.5197 (see D13 and the migration plan).

### D2 — Label fonts unchanged; nothing renders below 8 pt

The 10 pt in-cell font stays, exactly as the two pinned regressions require; the
problem is cell size, not font size. A label the cell cannot hold is annotated or
revealed instead of shrunk. The old terminal clamp that could derive 6 pt text is
retired in the window (no reachable path renders under 8 pt).

### D3 — Fit is a computation, not a stored zoom

Fit zoom = `min(pane_w / rack_w_mm, pane_h / rack_h_mm) / PHYSICAL_POINTS_PER_MM`,
clamped to the supported range [0.25, 4.0], with the rack centred: offset such that
the rack's centre lands on the pane's centre. A rack too large even for the floor
does not fully fit; pan covers it. Measured at a 640 × 520 pt pane:

| Rack (master + 0.5 mm gaps) | Width | Fit |
|---|---|---|
| master+S10+P10 | 92.94 mm | 107 % (height-bound) |
| master+P2B8+P8S8 | 108.18 mm | 107 % |
| master+P2B8+B32 | 118.34 mm | 107 % |
| master+P8S8+B32 | 133.58 mm | 107 % |
| master+M4+P8S8 | 153.90 mm | 107 % |
| master+M4+B32 | 164.06 mm | 103 % |
| 3-module case | ≈173 mm | 98 % |
| large rack | 400 mm | 42 % |
| any 2-row 3 HE rack | 257 mm | 54 % |

*Why*: single-row racks up to ~165 mm then open at 100–107 %, so the common case
opens at true size and the zoom question mostly disappears; the alternative —
opening at the configured zoom anchored at the origin — is what crops racks today.

### D4 — Fit triggers and the manual-navigation latch

Fit applies on: view open, patch load, rack-spec change, pane resize, and the fit
key. Any manual zoom or pan sets a latch that suppresses the automatic triggers,
and the fit key clears the latch. *Why*: the spec requires a fitted open and
requires zoom and pan to stay available; without the latch a resize would discard
the user's navigation.

### D5 — Zoom range and the preset ladder

The supported range widens from [0.75, 2.0] to [0.25, 4.0] (`[physical] zoom`
sanitisation and the runtime clamp agree). The `+`/`-` ladder
`[0.75, 1.0, 1.5, 2.0]` is unchanged in meaning (multipliers of the 1:1 baseline)
and steps to the next entry in the direction of travel from the current zoom, so an
off-ladder fit value (98 %) is respected rather than snapped. *Why*: 400 mm racks
and 2-row racks need the lower end; the ladder stays predictable, and no new key is
needed for fit because D6 adds one.

### D6 — `f` fits, `a` cycles the label presentation

Both keys are free while the module UI pane holds focus (the graph's `f`/`a`/`h`
arms are graph-scoped; `s` is the skeleton, `p` the performance view, `R` the state
reset, `e` the editor). `f` applies D3 and clears the latch; `a` cycles
`off → auto → on` and persists the choice. *Why*: fit and label presentation are
presentation concerns of the same family as the existing toggles, and both need a
key rather than a config edit.

### D7 — Wheel: value on a cell, zoom off it, `Ctrl` always zooms

A wheel over an adjustable cell keeps its ±0.05 value adjust (and is newly wired
into the window, where today it is a no-op); a wheel over the pane but not over an
adjustable cell zooms about the pointer; `Ctrl`+wheel zooms about the pointer
anywhere. Sensitivity and per-event step match the graph canvas
(`exp(Δy × 0.01)`, clamped to `[1/1.5, 1.5]`). *Why*: the value adjust is the only
continuous control for knobs and faders and must not be taken away; the graph's own
convention (wheel = zoom) is mirrored where it does not conflict, and `Ctrl` is the
unambiguous escape hatch over a cell. *Alternative*: wheel-off-cell pans — rejected,
pan already has a dedicated gesture (D8) and arrows, and zoom-by-wheel is the app's
established gesture.

### D8 — Middle-drag pans; the frame report becomes production

The pane dispatch applies the paint routine's report instead of discarding it:
pan delta, zoom about the anchor, and the skeleton toggle. `handle_physical_frame`
loses its `#[cfg(test)]` gate. Publish the module-UI pane rect each frame so
overflow uses live geometry, and retire `physical_viewport` / `physical_rack_size`.

*Why*: three separate dead ends (dropped frame, test-only handler, test-only
viewport) are why the spec's pan and zoom contract does not exist at runtime; the
fix is to reconnect what is already written and tested.

### D9 — `CellSpec.label_clipped`, computed where the budget is computed

The compact, composed, and fader paint paths already decide whether a label fits;
they surface that decision as a `label_clipped: bool` on `CellSpec` (true when the
label is non-empty and would be clipped or omitted) rather than recomputing a fit
test at the annotation site. *Why*: one budget, one answer — an independent test at
the annotation site would drift from what the cell draws and could double-label or
skip an element.

### D10 — Label presentation: `off` / `auto` / `on`, default `auto`

- `off` — in-cell labels only.
- `auto` — annotate exactly the elements whose `label_clipped` is true **and** whose
  label is explicitly set (a stored label or a patch preamble label); unaddressed
  elements never qualify (they carry no label). A cell whose label is nothing but the
  derived token name stays reveal-only even when clipped: on a dense patch those are
  dozens of callouts carrying no information the faceplate does not already imply,
  which is precisely the crowding `auto` exists to avoid (as-built refinement of the
  rule; `on` still annotates them). Annotations are placed by
  `performance::place_labels` (side-bound bands, leader anchors), so they are
  disjoint from the rack and from each other by construction.
- `on` — annotate every addressed element that carries a label.

When the deepest band would exceed 30 % of the pane on either axis, annotations
drop in a fixed order — LEDs and jacks first, then buttons, then switches, then
pots, then faders — and the focused element and the selected circuit's hardware are
never dropped. Dropped elements stay reachable by hover. The status reports
`N labels, M hidden`.

*Why `auto` as default*: it is the only mode that is both clutter-free at true size
(pots and short button labels render in-cell and get no callout, so nothing is
duplicated) and complete for the families that can never hold text in-cell (3–7 mm
jacks, LEDs, encoders, P8S8 faders). *Why a 30 % budget*: measured crowding — a
164 mm chain's 32-button module splits into ≈16 labels per side, which fits in
3 bands at the performance view's compact scale (≈26 % of pane height), while a
400 mm rack at the same label count needs 8 bands (≈2.7× pane height). The budget
turns "unusable when dense" into "denser patches show fewer labels", which is the
honest behaviour. *Alternatives*: a global zoom threshold switching between
in-cell and annotated presentation — rejected, families cross their fit points at
different zooms (a 9 mm button fits three characters at 1:1; a 3 mm LED never fits
below ≈2×), so any single cutoff mixes presentations arbitrarily within one patch;
abbreviations — rejected, labels are user strings with no canonical short form.

### D11 — Hover reveal in every mode

Hovering an addressed element draws a callout at the performance view's sizes
(14 pt monospace headline, 11.2 pt state row) anchored to that cell's edge, flipping
side to stay inside the pane, with the same leader line; leaving the pane clears it.
The pane's status suffix names the hovered element, falling back to the focused one.
*Why*: this is the spec'd reveal, it is the only mechanism that reaches the 3 mm
families at any zoom, and it costs one callout rather than a crowded layer.

### D12 — Status in the pane title, not a new strip

The module-UI pane's frame title carries the view state —
`Module UI · 98 % fit · labels auto · skeleton off` — ellipsized to the pane width,
in the existing muted chrome style; the `Pan x/y` segment appears only when the
offset is non-zero, `fit` only after a fit ran, and `labels` only when the mode is
not `off`. *Why*: `status_message` is only painted in the source viewer's footer
today, so the module UI has no status surface at all; reusing the pane title adds no
painted geometry, consumes no rack space, and cannot disturb the coincidence
contract. *Alternative*: a dedicated bottom strip (the memo's proposal) — a new
painted region that would either overlap the rack or shrink it; kept as a fallback
if the title proves too narrow in practice.

### D13 — Zoom migration: reset once, then honour the value

`[physical] zoom` values in existing configs were saved against the old baseline, so
on the first load after this change the stored zoom is ignored — the view opens
fitted — and a `[physical]` baseline marker is written on the next config save; from
then on the stored value is honoured (sanitised into [0.25, 4.0]). *Why*: the
maintainer's call — a 2.0 saved under the old scale would mean 2.5197× the apparent
size, and there is no way for a user to know their intent was relative rather than
absolute. The one-time reset is visible, explainable, and self-healing, where a
multiply-and-clamp would silently push most saved values to the 4.0 cap. *Migration
note*: the marker is additive, so an old config stays valid and a hand-written one
without the marker simply gets one fit.

## Risks / Trade-offs

- **Every scale-dependent assertion and artifact moves** → the recalibration sweep
  is an explicit task, the screenshots are re-rendered, and the perf gate is run at
  the new scale; the review point is the re-rendered module-UI shot.
- **Fit-on-open can zoom in past true size** (107 % for a short rack) → capped at
  4.0, reported in the status, and one `-` press returns to 100 %; the cap and the
  reported `fit` value keep it explicable.
- **`auto` can drop labels on a dense patch** → the drop order is fixed and
  documented, the focused element and the selected circuit's hardware are never
  dropped, hover always reveals, and the status names the hidden count so the
  behaviour is visible rather than mysterious.
- **The annotation layer could drift from the drawn cell** → it consumes the same
  `CellSpec` (`rect` + `label` + `label_clipped`) that the painter consumes, publishes
  no hit rects, and a regression asserts that switching modes leaves
  `component_rects` and the skeleton rects byte-identical.
- **Wheel-zoom about the pointer fights the value adjust** → the hit cell decides,
  documented in the Module UI help table, with `Ctrl` as the override; a regression
  pins that a wheel over a fader changes only its value.
- **`status_message` still does not surface outside the title** → out of scope; the
  graph and optimizer continue to report through their own paths, and the pane title
  is the module UI's surface for this change.
- **A very wide rack cannot fully fit at the 0.25 floor** → pan and the fit key
  remain the answer; the status reports the clamped fit so the user sees why.

## Migration Plan

1. Land D1 (scale) together with D13 (zoom reset marker) so no config value is
   interpreted against the wrong baseline.
2. The `[physical] labels` key is additive with the `auto` default; `[labels]`
   (resolution policy) is untouched.
3. `labels.toml` and patch files are untouched — nothing about resolution changes,
   only presentation.
4. Rollback is a revert: no persisted data is migrated destructively, and the only
   written change is the baseline marker plus a possibly re-saved zoom.
