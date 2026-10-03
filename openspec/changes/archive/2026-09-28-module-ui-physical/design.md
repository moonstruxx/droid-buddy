# Design

## Context

See proposal.md — Why. Two surfaces render controllers today: `gui/panels.rs::panels_spec` (generic portrait grids over `patch.hw_components`, LEDs included as cells) and `gui/physical.rs::paint_physical` (faceplate positions from `controller_geometry.json` via `PhysicalLayout`). Both share `paint_cell`/`CellSpec`. `ComponentState` is `Off | On | Value(f32)` — no colour. `controller_geometry.json` carries per-controller grids and `element_cells`, including LED grids and notes ("LED inside each button", "square of 32", "touch plate with integrated RGB LED"); its pitches and margins are marked *assumed*, counts and arrangements are manual-documented.

## Goals / Non-Goals

**Goals:**
- One module surface: the physical view, with every Panels interaction moved onto it.
- LED binding and colour modelled once, below the renderer, so the graph, source pane, and module UI agree.

**Non-Goals:**
- Exact millimetre fidelity (pitches stay assumed).
- DB8E display content, USB/MIDI ports, faceplate artwork.
- Changing the skeleton presentation beyond removing LED cells.

## Decisions

**D1 — Retire Panels rather than restyle it.** `ViewType::Panels` and `gui/panels.rs` are deleted; `BIG_CAROUSEL` becomes `[Graph, Physical]` and `PaneLayout::default()` puts `Physical` in `BigLeft`. Alternative (Panels as a re-scaled physical render) keeps two code paths and two hit-test sources for the same thing.

**D2 — LED binding lives in the model.** A pure resolver maps each LED token to its owning element per controller: explicit patch pairing (`led`/`ledN`) first, then the positional default from the controller's LED grid (same index as the element grid for B32/P2B8/P4B2/M4/P8S8; per-encoder blocks of 32 for E4/DB8E; per-jack for master/G8/X7). The renderer asks the model "which LEDs belong to this element" and never draws an LED on its own. Rationale: the physical design decides LED ownership, not the patch (project rule), and one resolver keeps panels-era `HwComponent.led` semantics consistent.

**D3 — LED colour is model state.** The LED state gains an optional RGB colour, set by the patch through the colour registers for that LED (to be confirmed per device in task 1.1). A per-controller capability (`rgb` / `white`) decides whether colour is rendered. White-only devices render brightness in the theme's LED white; RGB devices with no colour written render the device default. Colours come from the patch, not the theme — the one intentional exception to theme-token-only colouring, so the design-system rule gets an explicit carve-out for patch-driven LED colour.

**D4 — Unused elements come from geometry.** The rack model enumerates elements from `controller_geometry.json` `element_cells`, not from `hw_components`; patch tokens attach state and labels to those cells. Unmatched cells render dimmed (theme `muted`) without a label.

**D5 — Master replaces CV I/O.** The master/master18 entries already in `controller_geometry.json` become rack modules at chain position 0. The CV I/O grouping is removed.

**D6 — Labels keep the existing resolver.** Labels resolve through `Patch::display_label(token, effective_shift, layers_enabled, max_shift_layer, store)` exactly as today; the element draws the ellipsized label and the hover tooltip + status bar show the full one.

**D7 — Fit-width seeding.** On open (and on patch load) the physical zoom is set so the rack width fits the pane, using the published pane size (ADR 35 pattern, like `graph_canvas_px`). Presets remain for `+`/`-`.

## Risks / Trade-offs

- **Interaction regressions when Panels goes.** Panels owns keyboard navigation, the `m` latch target, the `e` label target, and ~20 handler tests. Mitigation: task 3.2 lists every interaction and task 4.1 tests each on the physical view before `panels.rs` is deleted.
- **Unverified LED register details.** Colour-register mapping per device is not in `controller_geometry.json`. Mitigation: task 1.1 verifies against the manual/RAGFlow before 1.2 builds on it; unverified devices fall back to brightness-only.
- **Readability at fit-width.** A wide rack at fit-width makes elements small and labels short. Accepted: hover/status show full labels, zoom is one key away.
- **Archive of removed capabilities.** REMOVED deltas that empty `controller-panels` and `module-scaling` will fail archive validation (as with `pane-class-layout`). Mitigation: at archive, delete those two main specs directly and drop their delta folders.
- **Docs.** ADR 10, 27, 33 and DESIGN.md panel sections become wrong; regenerate docs after archive.
