# Design

## Context

See `proposal.md` - Why. The current state shaping the approach: the
downstream path for unaddressed cells is fully built and dead. `CellSpec`
carries `component_index: Option<usize>` and `dimmed`
(`src/gui/physical.rs:85-106`); `physical_spec` branches on `ci <
module.components.len()` to render dimmed/unlabelled `None` cells (290-300,
386-409, including the CV-vs-`·` glyph choice); `paint_panes` publishes only
`Some` indices (`src/gui/mod.rs:613-621`); the highlight closure is
panic-safe via `is_some_and` short-circuit (422-426). The sole gap is
emission: `rack_geometry` iterates `module.components` only (1265-1284), so
`ci` never exceeds the component count. Modules already carry every
geometry-declared cell (`PhysicalModule.cells: HashMap<family,
Vec<ElementCell>>` with element numbers + kind letters, `cell_for` +
`token_element` for token matching).

## Goals / Non-Goals

**Goals:**

- Every geometry-declared cell renders: addressed cells as today,
  unaddressed cells dimmed and unlabelled with no hit rect.
- Fold-rule LEDs never regress to standalone cells.
- Full-front patches render byte-identical specs (no unaddressed cells, no
  diff).

**Non-Goals:** everything in the proposal's Non-goals.

## Decisions

### D1 — Emit unaddressed cells from `rack_geometry`, ordered deterministically

After the per-component loop, walk each module's `cells` family map and emit
cells whose `(kind_letter, element)` is not among the module's component ids
(matched via the existing `token_element`), with `ci = components.len() +
ordinal` and the I/O `PortMark` mapping reused. Order: family key sorted,
then row, then col — `HashMap` iteration order is per-process random and
would scramble specs, paint order, and the coincidence proof.

*Why:* one emission site keeps skeleton and full coincident (both read
`geom.cells`); the spec's `ci`-based branch needs no new discriminant.

*Alternatives:* emit from `physical_spec` (it lacks the geometry walk and
would duplicate `cell_for` logic); change the `geom.cells` tuple shape to
carry tokens (churns the skeleton path for no behavioural gain — the spec
recovers tokens via D2).

### D2 — Recover the token via the cell label for fold suppression

`ElementCell.label` already holds the token-ish id (`"B1.3"`). For an
unaddressed cell, the spec checks `fold_shape_for_controller(module
geometry_key)`: when `Some` (P2B8/B32/M4) and the cell is an `L`-family cell,
suppress it (fold rule — its face, addressed or dimmed, already shows the
floor fill). All other unaddressed cells render dimmed standalone.

*Why:* no tuple change, no new plumbing; the suppression reuses the exact
per-controller rule task 3.2 introduced, so painter and rule cannot disagree.

*Alternatives:* a `PhysicalModule::unaddressed_token(ci)` helper (extra API
for what the label already carries); suppressing only when the face exists
(wrong — a dimmed unaddressed face plus a standalone dimmed LED would double
-render one element).

### D3 — Pure enumeration in `physical.rs`, painter-thin consumption

`PhysicalModule::unaddressed_cells` (name TBD) returns the addressed-excluded
cells; `rack_geometry` maps them to screen rects with the same `cell()`
closure and clamping as addressed cells (adjoined-cell overlap clamping
applies unchanged).

*Why:* the enumeration is unit-testable without egui; the gui layer stays a
thin mapper, matching the codebase's pure-model/gui-paint split.

### D4 — No interaction change whatsoever

Unaddressed cells publish no rect, so hover/click/navigate/selection/label
editor/modifier latch cannot observe them; `j`/`k` order is unchanged
(`component_rects` shape and order preserved — emission appends per module
after addressed cells, and publication filters).

*Why:* the spec's "dimmed and unlabelled" plus "never hit" is exactly the
existing downstream contract; anything more (e.g. focusable dimmed cells)
would be a new interaction, out of scope.

### D5 — Bare full fronts stay byte-identical

Bare `[p2b8]`/`[b32]`/`[m4]` synthesize complete B/L twin sets, so no
unaddressed cell exists and specs render exactly as today. The E4 bare front
(E4+B4+L4) gains 124 dimmed ring segments — intended, spec-required, and the
screenshot re-bless proves it rather than hides it.

## Risks / Trade-offs

- [124 new dimmed segments could clutter the E4 face] → intended per spec
  ("Unused elements are drawn" + E4-rings requirement); dimmed + unlabelled
  keeps them quiet; the re-blessed shot is the visual review gate.
- [Family-map walk may surface geometry cells the loader never matched
  before (e.g. fallback modules)] → fallback modules have empty `cells`
  maps, so nothing emits; `element: None` cells (e.g. `"USB"`) never match a
  token pair and are skipped by construction.
- [Deterministic order depends on sort keys] → family key + row + col is
  total for grid cells; pinned by an order-assertion test.
- [Performance-view compact rack] → it reuses spec cells; dimmed cells ride
  along. If the performance overlay misplaces them, that surfaces in its
  paint tests (watchpoint for the 2.2 lane, not a separate task).

## Open Questions

- None that change what's built.
