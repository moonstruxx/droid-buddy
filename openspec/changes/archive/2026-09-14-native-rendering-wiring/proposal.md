## Why

Five surface modules (`physical`, `panels`, `viewer`, `picker`, `overlays`) are gated behind `#[cfg(test)]` in `src/gui/mod.rs` — their draw routines exist and have headless egui tests, but nothing at runtime dispatches to them. Only `graph::paint_scene` is live in `EguiSurface::paint`. The `?` help modal, controller panels, physical 1:1 view, source viewer, picker/favourites, and all overlays (validation/diff/optimizer/label) therefore produce no pixels at runtime despite spec-pass status.

## What Changes

- Remove the `#[cfg(test)]` gate on `mod physical`, `mod panels`, `mod viewer`, `mod picker`, `mod overlays` in `src/gui/mod.rs` so they compile unconditionally.
- Wire each surface's draw routine into `EguiSurface::paint` alongside the existing graph canvas, respecting the same tiled/pane dispatch the specs require (panels/physical/viewer/picker/overlays each get a pane/overlay rect from `App` state).
- Keep all existing headless tests passing; add a runtime smoke per surface (egui `Context::run` → `FullOutput` shapes non-empty) to prevent regression.
- Keep the `native-rendering` spec green (`openspec validate --specs`).

## Non-goals

- No new surfaces, no theme changes, no graph-solver optimization, no hardware bridge. Strictly wiring existing routines into the shell.
