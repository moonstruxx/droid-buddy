# Proposal

## Why

The module UI is the app's only hardware surface, and since `module-ui-labels` a
cell shows its label only when the *fixed* 10 pt string happens to fit: a 9 mm
button cell is 34 pt and budgets five characters, a 7 mm jack two, a 3 mm LED
none, and a 4.2 mm fader never — its track plus gutter leave under 3 pt of text
column. Everything else is pushed to an annotation layer that annotates **only
labels the user wrote**, only when the cell clips them, and then drops them in
family order once a band passes 30 % of the pane. On a 400 mm rack the fit zoom is
0.66, so a 9 mm button has six characters of total cell area and the state glyph
consumes two: naming the control under your finger is invisible in the default
view.

The previous change recorded two non-goals this one overturns: *no per-element
font scaling* and *no rendered text below 8 pt*. Both were right while the only
lever was shrinking one fixed line, and wrong now that the alternative is a label
nobody sees. Fitting is not a zoom problem — measured against the real cell sizes,
a descending ladder of 10 → 6 pt with word wrap puts the typical written label's
fit zoom at **0.75–1.4×**, against the 2–4× a single 10 pt line would need.

Three behaviours are unimplemented or missing:

- `module-ui` "Shift-aware element labels" — a label that does not fit at the
  fixed font is dropped, not fitted; nothing wraps, nothing shrinks.
- `physical-scale-model` "Physical cell rendering contract (compact-only)" — pins
  the font to 10 pt at every zoom, which is exactly why small cells cannot hold
  text.
- `module-ui` "Fit-width default" — fits the *rack* only. The fit ignores whether
  the result is readable, so the default view is the least legible state the view
  can be in.

## What Changes

- **Per-cell label fitting.** `label_layout(cell) -> Option<LabelLayout>` replaces
  `label_is_clipped`: a descending font ladder (10, 9, 8, 7, 6 pt, monospace),
  greedy word wrap, a hard break only for an over-long word, and an ellipsis on
  the last line only when even the floor cannot hold the whole label. It is pure
  and deterministic, computed once when the spec is built, and it is the single
  source the painter, the annotation layer and the fit clamp all read — the
  re-derived budgets that let annotations drift from what is drawn disappear with
  the boolean.
- **The in-cell state text yields.** A cell first tries to fit its label with the
  state row reserved; when no ladder step fits alongside it, the label takes the
  cell and the state row is dropped. Numeric state stays available in the hover
  reveal and the pane's status line, and fader/LED state stays graphical.
- **Faders stack.** A tall-narrow fader cell (P8S8, 4.2 × 22 mm) narrows its track
  to 40 % of the cell width and stacks the label one glyph per line in the
  remaining column — the only orientation in which a 16 pt-wide cell holds eleven
  characters.
- **Composed button faces keep their shape.** P2B8/B32/M4 keep their LED face and
  give the label the band beneath it (two lines when the band's height allows); a
  label that cannot fit there at 6 pt stays an annotation candidate rather than
  eating the face.
- **The fit becomes labels-first.** The fitted zoom is the greater of the rack fit
  and the label fit — the smallest zoom at which every *written* label's cell
  yields a layout — clamped to the supported range. Cells that cannot fit even at
  `ZOOM_MAX` (3 mm LEDs, and any leftover at extreme zoom) never lift the fit and
  stay annotation candidates. The rack may overflow and pan: overflow panning
  already exists, and the fit latch, its triggers and `fit_offset` centring are
  unchanged.
- **Two explicit fits.** `f` fits labels (the new default), `Shift+F` fits the
  rack; the status bar and the module-UI pane title say which ran
  (`fit(labels)` / `fit(rack)`) through one `physical_fit_mode` recorded with the
  fitted zoom.
- **Annotation rule restated.** `auto` annotates exactly the written labels that
  have no in-cell layout, which after the clamp is a near-silent layer; `on` and
  `off` keep their present behaviour, including `on`'s 30 % drop.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `module-ui`: element labels are fitted per cell instead of dropped at a fixed
  font size; the fit is labels-first; the annotation rule is restated against a
  cell layout rather than a clip flag; the composed face names the label band it
  leaves.
- `physical-scale-model`: the compact-cell contract replaces the fixed font and
  the 8 pt floor with the ladder, the wrap, and the stacked fader text; the 1:1
  view's fitted-open contract gains the label clamp and names overflow as the
  intended consequence.
- `keybinding`: the physical-view navigation requirement gains the two fit
  meanings (`f` labels, `Shift+F` rack) and their status wording.

## Impact

- Touched: `src/gui/physical.rs` (`LabelLayout`, ladder/wrap/stack, the three cell
  paint paths, the annotation candidate filter, `CellSpec`), `src/physical.rs`
  (`label_fit_zoom`), `src/app.rs` (fit clamp, `physical_fit_mode`, status),
  `src/handler.rs` (`f` and `Shift+F`), `src/gui/mod.rs` (pane-title segment),
  `src/help.rs`, `src/regression.rs`, `tests/perf_gate.rs`, `fixtures/`, `docs/`,
  `tools/screenshots/`.
- Visible behaviour: at the default fit every written label is drawn inside its
  own cell, smaller than today and wrapped; numeric state text disappears from
  cells that need the room; a large rack opens zoomed past the pane and pans;
  `Shift+F` restores the whole-rack view with labels shrinking away.
- Unchanged: hit rects, skeleton rects, geometry JSON, LED folding, the
  performance view's compact mapping, the annotation placement engine, the
  `[physical] labels` semantics, and every config key — no migration, no new file,
  no new dependency.
- Review point: the 6 pt floor is a deliberate legibility trade and losing the
  in-cell state row is a deliberate information trade; both belong in the spec
  text, not only in the code.

## Non-goals

- No label abbreviation, synthesis, or in-cell display of derived token names
  ("Button B1.3" stays hover- and annotation-reachable only when explicitly set).
- No legend pane, gutter expansion, scrollbar, or user-facing "zoom for labels"
  mode beyond the fit clamp.
- No rotated text: stacking glyphs vertically is the only non-horizontal form.
- No change to the annotation placement engine (`performance::place_labels`), to
  `on`'s 30 % drop budget, or to the performance view.
- No change to `controller_geometry.json`, `rack_geometry.json`, published hit
  geometry, or the skeleton presentation.
- No persistence of the fit mode, no new config key, no `.ini` mutation.
- No hand-edits of ARCHITECTURE.md or DESIGN.md; those regenerate at archive.
