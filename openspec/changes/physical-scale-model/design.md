# Design — physical-scale-model

## Context

The main view is a schematic of logical panels; the goal is a 1:1 physical scale model. One geometry must drive two presentations (skeleton reference + full view) so fidelity is provable. All geometry is pure (no terminal dependency), mirroring `geometry.rs` / `graph.rs`.

## Decisions

### D1: Millimeters as the single grid unit

Every module width derives from HP (1 HP = 5.08 mm, manual spec data). Element cells are positioned in mm on a per-controller faceplate grid. Screen mapping happens only at render time. This keeps the model a true scale model across master, controllers, and expanders, and makes cross-controller gaps physical, not cosmetic.

### D2: One geometry, two renders (skeleton reference + full view)

`PhysicalLayout` is the single source of truth. Skeleton mode draws only module outlines + element cells ("the important visual characteristics"); the full view draws components onto the same cells. Verification = coincidence: every full-render element rect equals its skeleton cell at the same scale/offset. This directly implements the user's requirement to verify against a version containing only the important visual characteristics.

### D3: Chain order from patch declaration order

The controller chain is built in the order the patch declares its hardware (mirroring how `scan_hw_tokens`/`HwComponent` order already works). Repeated instances of a controller (e.g. two `[p2b8]`) become separate side-by-side faceplates at their real width — replacing the current "Panel contains modules" sub-blocking in the physical view.

### D4: Aspect-compensated mm→chars mapping

Terminal cells are ~2:1 (wider than tall). The mapping uses two factors (columns/mm vs rows/mm) so physical proportions survive rendering: width_mm × cx columns, height_mm × cy rows with cy ≈ 2·cx. The rack is laid out once in mm; the renderer applies the factors, zoom, and pan offset to produce screen rects.

### D5: Viewport model — pan offset + zoom

State holds `physical_offset: (f32, f32)` in screen units and `physical_zoom` (mapped from the existing `+`/`-` scale presets). Renders compute screen_rect = (mm_pos × factor × zoom) − offset. `component_rects` is published from the same formula, preserving the renderer-owns-geometry contract (ADR 4/22). Wheel pans when the rack overflows; wheel on a knob/fader cell still adjusts values when no overflow forces panning.

### D6: Data-driven geometry with load-time validation

Per-controller specs live in a JSON data file (`controller_geometry.json`, `rack_geometry.json` pattern, resolved via `CARGO_MANIFEST_DIR` or embedded). Unknown/missing controller → fallback width (5 HP average) + warn; malformed data → fallback, never panic. B32 orientation follows the manual (4 cols × 8 rows), resolving the conflict with the current 8×4 assumption.

### D7: Skeleton mode is a render presentation, not a new surface

No new full-screen surface: the skeleton toggle switches the main view's presentation (full ↔ skeleton) of the same layout — trivial to snapshot, no handler priority changes beyond the toggle key.

### D8: Verification harness

- Unit: skeleton cell == full-render rect for every token in fixtures (coincidence).
- Snapshot: skeleton + full frames for physical-layout scenarios (classic/mono/terminal).
- Gallery: skeleton | full rows in the matrix (visual proof per project requirement).
- All deterministic (TestBackend, no RNG) — same guarantees as the existing visual matrix.

## Non-goals

- No DB8E display rendering, no X7/G8/R2M-R2C element-level rendering beyond the data.
- No hardware bridge; no new dependencies; no async.
- The wrapped-panel main view is replaced, not retained as a mode.
- No per-element millimeter detail beyond the geometry data.
- No latency coloring in the physical view.

## Risks

- **Data accuracy**: element positions come from manual figures/specs; fallback widths cover gaps. Coincidence verification proves internal consistency, not physical truth — external accuracy is a data-quality task.
- **Overflow UX**: very wide racks need pan/zoom to be discoverable; status hints and the existing render-outlier hint channel mitigate.
- **Snapshot churn**: the main-view face change invalidates existing panel snapshots — acceptable, `insta` review workflow applies.