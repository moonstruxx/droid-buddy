# Proposal — physical-scale-model

## Why

The main view groups components into logical controller panels wrapped into uniform rows — a schematic, not a picture. Module sizes (HP), the gaps between controllers, real element positions on each faceplate, and the relative placement of master vs. controllers are lost, so a user cannot map a screen cell to the hardware in front of them. This change makes the main view a true 1:1 physical scale model and adds a geometry-only reference render to prove fidelity.

## What Changes

- **Per-controller physical geometry data**: mm-derived module specs (HP → 5.08 mm per HP), element cell positions per element family, chain gaps; acquired from the DROID manual + knowledge base; resolves the B32 orientation conflict (manual 4×8).
- **Pure grid model (`src/physical.rs`)**: `PhysicalLayout::build(patch)` — ordered controller chain → module rects + element cells in mm; cell lookup by hardware token; repeated circuit instances become side-by-side faceplates.
- **User-defined rack/case**: configurable case — ordered rows (HE + HP each), optional top-mount and side-mount TE sections; controllers auto-pack into rows by width in chain order with per-module row overrides; the case outline and fold bars at row boundaries render in both views.
- **Skeleton reference mode**: geometry-only render (module outlines + element cells — only the important visual characteristics), toggleable, theme-tokened; the validation ground truth.
- **1:1 main view (BREAKING for the wrapped-panel view)**: components render onto the grid cells, replacing the wrapped-panel main view; uniform mm→chars mapping with aspect compensation, zoom levels, pan/scroll for overflow; LED-folding + boxed cells and the `component_rects` hit-testing contract preserved.
- **Verification**: coincidence tests (every full-render element rect equals its skeleton cell) plus gallery skeleton | full side-by-side rows.

## Capabilities

### New Capabilities

- `physical-scale-model`: physical grid model (per-controller mm geometry, chain order, element cells), rack/case model (rows, TE mounts, auto-pack + per-module overrides), skeleton reference render, 1:1 main view with fold-bar rendering, pan/zoom interaction, and geometry-coincidence verification.

### Modified Capabilities

- `controller-panels`: the main panel layout becomes the 1:1 physical layout (module sizes, gaps, element positions) instead of wrapped logical panels; multi-circuit panels become side-by-side faceplates.
- `module-scaling`: scale presets become physical zoom over the rack.
- `keybinding`: pan/zoom/skeleton-toggle keys.
- `mouse-interaction`: mouse-wheel pan/scroll over the rack.
- `visual-validation`: gallery matrix gains skeleton | full proof rows.

## Impact

- `src/physical.rs` (new) — pure grid model; no terminal dependency.
- `controller_geometry.json` (new data file, `rack_geometry.json` pattern) — per-controller mm specs.
- `src/ui.rs` — skeleton renderer + 1:1 main renderer (replaces the `render_patch_grouped` wrapping path).
- `src/app.rs` / `src/handler.rs` — physical-view state (pan offset, zoom, mode), keys.
- `src/config.rs` — optional `[physical]` + `[physical.rack]` defaults (zoom, pan/scroll, rack rows / TE mounts / assignments).
- `src/theme.rs` — skeleton tokens in classic/mono/terminal.
- `src/regression.rs` + `fixtures/**` — coincidence proof tests + snapshots.
- `src/bin/snapshot-gallery.rs` — skeleton | full rows in the visual matrix.
- Delta specs: `controller-panels`, `module-scaling`, `keybinding`, `mouse-interaction`, `visual-validation`.

## Non-goals

- No DB8E display rendering, no X7/G8/R2M-R2C element-level rendering beyond the geometry data.
- No hardware bridge; state stays simulated.
- The wrapped-panel main view is replaced, not kept as an alternate mode (YAGNI).
- No per-element millimeter detail beyond what the geometry data provides.
- No latency coloring in the physical view.