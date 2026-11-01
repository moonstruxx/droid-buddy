# Design

## Context

See `proposal.md` — Why. The constraints that shape the approach:

- Cell geometry is millimetres turned into points by one isotropic factor:
  `PHYSICAL_POINTS_PER_MM = 3.779527559`, so at 100 % zoom a millimetre is 3.78 pt
  and a 9 mm button cell is 34 pt. The cell sizes that matter here are fixed by
  `controller_geometry.json` and are not negotiable: buttons 9 mm (P2B8, B32, P4B2)
  and 12 mm (M4), pots 12 mm, switches 5–12 mm, jacks 7 mm, LEDs 3 mm, the P8S8
  fader 4.2 × 22 mm, the M4 fader 7 × 62 mm.
- Label text is monospace in every cell path today at `CELL_LABEL_SIZE = 10.0`
  with `MONO_ADVANCE = 0.6`, so a character is 6 pt and budgets are computable
  without measuring rendered text. The one exception is the fader label, which
  uses a proportional font clamped to 8–13 pt derived from cell height.
- `label_is_clipped` (module-ui-labels D9) re-derives each paint path's character
  budget by hand and answers one boolean. It already duplicates three budgets —
  compact (glyph + space + label), composed (`h − label_line ≥ 0.5 w`), fader
  (track reserve + height-derived font) — and only the annotation layer consumes
  it.
- The fitted view is `physical::fit_zoom(rack_w_mm, rack_h_mm, pane_w, pane_h)`
  (the smaller of the width and height ratios, clamped to
  `[ZOOM_MIN 0.25, ZOOM_MAX 4.0]`), applied by `App::fit_physical_view` with
  `fit_offset` centring and a manual-navigation latch. The pane title and
  `physical_status_hint` already report the zoom and a `fit` marker.
- `CellSpec` is built once per frame in `gui::physical::physical_spec`; the painter,
  the annotation candidate filter, the hover callout and (via `paint_panes`) the
  published `component_rects` all consume it. Nothing in the fit path may perturb
  cell rects: the coincidence contract between drawn cells, hit rects and the
  skeleton is regression-pinned.
- Specs currently forbid what this change needs: `physical-scale-model` pins the
  label font across zoom presets and requires "no text below 8 pt";
  `module-ui-labels` recorded "no per-element font scaling" as a non-goal.

## Goals / Non-Goals

**Goals:**

- Every written label is drawn inside its own cell at the default view, and stays
  reachable (annotation, hover) wherever no font step can fit it.
- One pure function decides what a cell's label looks like, and everything that
  needs the answer reads it instead of re-deriving it.
- The default fit is a readable state; the whole-rack overview becomes an explicit
  request instead of the only fit there is.
- No perturbation of geometry: drawn cell rects, published hit rects and skeleton
  rects stay byte-identical to today.

**Non-Goals:**

- No rotated text, no legend, no scrollbar, no abbreviation or label synthesis.
- No per-family uniform sizing (see Risks — per-cell ladder first, uniform only if
  the mixed sizes prove noisy in review).
- No change to the annotation placement engine or to `on`'s drop budget.
- No persistence of the fit mode and no new config key.

## Decisions

### D1 — `label_layout` replaces `label_is_clipped`

One pure function in `src/physical.rs` — the model layer, because the fit clamp in
`App` needs the same answer and `app` must not depend on `gui`:

```
LabelLayout { font: f32, lines: Vec<String>, stacked: bool, state_row: bool }
LabelBox { w: f32, h: f32, glyph_chars: usize, path: CellTextPath, has_state: bool }
CellTextPath { Compact, Composed, Fader }
label_layout(box: &LabelBox) -> Option<LabelLayout>
```

`LabelBox` is the cell's drawn size in points as the painter will use it, plus the
three facts that change the budget (the glyph prefix, which paint path applies,
whether a state string exists). No egui type crosses the boundary, so the function
is unit-testable without a paint and usable from the fit clamp at any candidate
zoom. `gui::physical` builds the `LabelBox` from `CellSpec`, stores the resulting
`LabelLayout` on `CellSpec` in place of `label_clipped: bool`, and paints it;
`App` and `physical::label_fit_zoom` feed it scaled rects. `label_layout(..).is_none()`
is the annotation predicate. Rationale: five font steps × wrapped lines × two orientations is far
too much duplicated arithmetic for the three hand-written budgets to stay in step
with the painter — the boolean exists precisely because the budgets already drift.

### D2 — Ladder, wrap, hard break, ellipsis

Try font steps descending `10, 9, 8, 7, 6`. For each step, compute the cell's
available text box for that path, greedily wrap at spaces, break a word only when a
single word exceeds the line width, and accept the first step whose every line fits
both width and height. If no step fits, try again with a smaller text box set (drop
the state row, D4) before giving up. Only at 6 pt is the label ellipsized — the
ellipsis means "no step can hold the whole word", not "the first try was tight".

Measured against the real cell sizes, this is what makes the change affordable: a
13-character label on a 9 mm button fits in three 6 pt lines at a zoom of ~0.66,
where a single 10 pt line would need 2.3×. The ladder costs at most five attempts
per cell and runs once per frame over ~200 cells.

### D3 — Six-point floor, monospace everywhere

The floor drops from 8 pt to 6 pt because 6 pt is the smallest size at which a
3.6 pt character advance stays inside a 7 mm jack at 100 % zoom. Choosing mono for
the fader label too (today proportional) makes every budget a multiplication of
`0.6 × font` instead of a per-glyph measurement, which is what keeps the function
pure and testable without egui.

### D4 — The state text yields to the label

Order of preference per cell: fit the label with the state row reserved; if no step
fits, fit the label across the whole cell and drop the state row. Rationale: the
state is also carried by the hover reveal, the pane's status subject, and the
graphical channels (fader bar, LED fill, composed face outline); the label has no
second carrier inside the cell. The state row keeps its own font rule when it
survives (proportional, `max(9, 0.8 × label font)`).

### D5 — Faders narrow the track and stack glyphs

The P8S8 fader cell is 4.2 mm wide: at 100 % zoom the track's `clamp(2, 10)` claims
10 of its 15.9 pt, leaving 2.9 pt — less than one 6 pt character. The fader path
therefore narrows the track to `min(0.4 × w, 10)` and stacks the label one glyph
per line in the remaining column at the largest step whose glyph width and total
column height fit. At 100 % zoom that is a ~6.4 pt track, a 6.5 pt column and a
6 pt stack holding ~10 characters in the cell's 83 pt height. The M4 fader
(7 × 62 mm) is wide enough for a horizontal line and keeps one.

### D6 — Composed button faces are never eaten

P2B8/B32/M4 keep their LED face. The label gets the band beneath the shape —
`h − 0.5 w` of usable height, drawn from the cell's bottom edge over the face's
lower arc, which is how the existing composed path already places it — at the
largest step that fits, in up to two lines when the band's height allows. When no
step fits, the cell stays face-only and the label remains an annotation candidate.
Rationale: the face is that controller's LED state channel; trading it for text
would remove a different signal instead of adding one.

### D7 — Fit = max(rack fit, written-label fit), capped

`physical::label_fit_zoom(requests) -> Option<f64>` returns the smallest zoom at
which every written label is **drawn whole**: a bounded, deterministic ascending
scan over the same `label_layout` applied to the cell rect scaled by the candidate
zoom, capped at `ZOOM_MAX`. "Drawn whole" is the predicate, not `Some(layout)` —
the floor-ellipsis path returns a layout from a far lower zoom (the 9 mm button
takes `Some` at 0.376 where it paints a bare `…`), and accepting that would make
the clamp a no-op. Requests that still cannot be drawn whole at `ZOOM_MAX` are
ignored — they stay annotation candidates and never inflate the zoom. Then

```
fit = max(fit_zoom(rack, pane), label_fit_zoom(...)).clamp(ZOOM_MIN, ZOOM_MAX)
```

Only *written* labels participate (`Patch::explicit_hw_label`), matching the
annotation rule and keeping derived token names from zooming the view. The whole
fit uses one `LabelBox` shape: `glyph_chars` is 0, because the module UI paints the
state glyph as a corner marker rather than charging it against a label line — the
request type keeps the field so fit and paint cannot silently disagree, and the
module-UI caller always passes the same 0 the painter passes.

Consequence worth stating: the ignore set is not "3 mm LEDs" — a 3 mm cell scales
with the zoom and holds a short written label at ~2.0, so a labelled LED *does*
lift the fit (up to the cap). Only labels that no supported zoom can draw whole
remain annotations. If a single dot's label lifting the view to 2× proves jarring
in review, a lift-factor bound relative to the rack fit is the knob to add.

### D8 — `f` fits labels, `Shift+F` fits the rack

`App::fit_physical_view` gains a fit mode and records it with the fitted zoom
(`physical_fit_mode: FitMode { Labels, Rack }`). `f` performs the labels fit
(D7), `Shift+F` performs the rack fit (today's behaviour, including the clamp to
the supported range and no label lift). The mode is session state, never
persisted: it names what the last fit did, and a restart fits labels again.
Status and pane title report `fit(labels)` / `fit(rack)` so a user who is panning
a large rack can see that the zoom is the readable one, not a bug.

`Shift+F` is free: the global handler binds `Char('f')` for the physical pane and
`Char('F')` only inside the file picker (favourites).

### D9 — The annotation rule keeps its intent and goes quiet

`auto` becomes: annotate exactly those addressed elements whose label is
explicitly set and whose cell cannot draw it whole at the current zoom. With the
fit clamp, at open that set is only the labels no supported zoom can serve — a
very long label on a tiny cell, of any family — so the crowding the 30 % budget
was written to contain mostly disappears, and the budget stays in place for `on`
and for manual zoom-in. `on` and `off` are untouched.

### D10 — The legibility floor is a documented trade

`physical-scale-model`'s "no text below 8 pt" becomes "no text below 6 pt", and
the label requirement states the ladder explicitly. A silently repealed contract
is worse than a documented one: 6 pt monospace is roughly 8 px on a 1× display —
legible for a few characters, tiring for a paragraph — which is why the ladder
stops there and why a cell that cannot take 6 pt stays annotation/hover territory
rather than shrinking further.

## Risks / Trade-offs

- **Mixed font sizes across neighbouring cells read as noise.** Five steps within
  one faceplate can look ragged. Mitigation if review disagrees: quantize the
  ladder per family (all buttons share a step) — deliberately not the first
  implementation, because it costs coverage on short-label cells.
- **6 pt is small text.** Accepted explicitly (D10) and reported in the docs; the
  alternative is a label that does not exist.
- **The default view no longer shows the whole rack on large patches.** This is
  the point of the clamp (the user asked for readable-by-default), and `Shift+F`
  plus the existing wheel/pan gestures make the overview one keypress away.
- **Losing the numeric state row.** Knob percentages disappear from cells that
  need the room; they remain in hover and the status subject. Fader and LED state
  are graphical and unaffected.
- **Per-frame cost.** `label_layout` runs for every cell each frame (word wrap over
  ~1–3 short strings). It replaces work of the same order (`label_is_clipped` plus
  the painter's own budgeting), so the expectation is neutral; task 3.3 measures it
  on the scale anchor before anyone tunes anything.
- **Fit thrash on edit.** Saving a longer label can lift the fit zoom on the next
  fit trigger. The latch means it does not fight the user mid-navigation; the
  fitted zoom is recomputed only on open, patch load, rack change, pane resize and
  an explicit fit.

## Migration Plan

- No data migration: no config key is added, renamed or re-interpreted
  (`[physical] labels` keeps `off`/`auto`/`on` with the restated `auto` meaning and
  no persisted fit mode).
- No `.ini` mutation, no new dependency, no crate feature.
- Documentation in the same change: `docs/views.md` (label fitting and the two
  fits), `docs/keys.md` (`f`, `Shift+F`), `docs/config.md` (the restated `auto`
  meaning), plus re-rendered shots.
- ARCHITECTURE.md / DESIGN.md regenerate at archive, not by hand.
