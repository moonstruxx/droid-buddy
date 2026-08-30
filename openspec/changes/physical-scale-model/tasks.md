# Tasks — physical-scale-model

## 1. Geometry Data

- [x] 1.1 Acquire + encode per-controller physical geometry <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [controller_geometry.json, tools/acquire_geometry.py] -->
      · from the DROID manual + knowledge base (droid-knowledge-base skill): per-controller HP width (master 8 HP, DB8E 6 HP, R2M/R2C 2 HP each, P2B8/P4B2/P10/S10/P8S8/B32/E4/M4/CV) and element cell positions/sizes in mm (button/pot/fader/encoder/switch/LED/CV per family); chain gaps between master and controllers; resolve the B32 orientation conflict to the manual's 4 cols × 8 rows; encode as `controller_geometry.json` (`rack_geometry.json` pattern, mm as the unit); each controller type carries an `he` (1|3) default (controllers are 3 HE)
      · verify: the data file covers every controller type the schema knows; all dimensions positive; element cells lie within their module rect; a small script cross-checks declared cell grids against token-family counts
- [x] 1.2 Geometry data sanity tests <!-- agent: horst-engineer.fast, depends_on: [1.1, 2.1], touches: [controller_geometry.json, src/physical.rs] -->
      · load-time validation: every controller type present, positive mm, cells within module rect, B32 resolved 4×8; malformed/missing entry falls back (5 HP) without panic
      · verify: `cargo test` new data-sanity tests pass

## 2. Grid Model

- [x] 2.1 Pure grid model module <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/physical.rs, src/lib.rs] -->
      · `src/physical.rs` (no terminal dependency, mirrors geometry.rs/graph.rs): `PhysicalLayout::build(&Patch)` → ordered controller chain from declaration order; per-controller module rects in mm; element cell lookup per HwToken; repeated circuit instances → separate faceplates (module_instance-aware); load geometry data with fallback + warn
      · verify: `cargo test` in-module unit tests (chain order, cell lookup per family, multi-instance faceplates, fallback)
- [x] 2.3 Rack/case model + row assignment <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/physical.rs] -->
      · `RackSpec` (rows: he/hp/label, top_mount_te, side_mount_te, assign map) + `RackLayout::pack(chain)` → modules into rows: auto-pack in chain order (row fills until the next module would exceed its HP, then next row), per-module override, out-of-range override → auto fallback; fold-line mm positions at row boundaries; default single-row case wide enough for the chain; pure (no config parsing — that is 4.4)
      · verify: `cargo test` in-module unit tests (auto-pack fill/overflow, override placement, out-of-range fallback, fold-line positions, determinism)
- [x] 2.2 Grid-model unit tests <!-- agent: horst-engineer.build, depends_on: [2.1, 2.3], touches: [src/physical.rs] -->
      · chain order matches patch declaration; element cells resolve per token family; two `[p2b8]` instances yield two faceplates at real widths; unknown controller falls back to 5 HP; deterministic across runs; rack packing: auto-pack fills row 0 then overflows to row 1, per-module override places correctly, out-of-range override falls back, fold lines at expected mm positions
      · verify: `cargo test` green; determinism asserted (identical layout on repeat)

## 3. Skeleton Reference

- [x] 3.1 Skeleton renderer + mode state <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/ui.rs, src/app.rs, src/handler.rs] -->
      · geometry-only render: module outlines + element cells, no labels/states; mode flag on App; toggle key in handler; rendered from the same PhysicalLayout the full view uses; publishes skeleton cell rects for the coincidence tests
      · verify: `cargo test` renders skeleton for a fixture patch; toggle switches presentation back and forth; no handler priority regressions
- [x] 3.2 Skeleton theme tokens <!-- agent: layout-designer-engineer.fast, depends_on: [3.1], touches: [src/theme.rs] -->
      · `physical_skeleton_*` tokens (module outline, element cell, port markers) in classic/mono/terminal, following the token-layer invariant (no `Color::` literals outside tests)
      · verify: tokens present in all three palettes; mono pairwise-distinct where they co-occur
- [x] 3.3 Skeleton snapshot coverage <!-- agent: horst-engineer.fast, depends_on: [3.1], touches: [src/regression.rs] -->
      · insta snapshots of skeleton frames for physical-layout fixtures (classic/mono/terminal)
      · verify: `cargo insta test --check` green after accepting new snapshots

## 4. 1:1 Main View

- [x] 4.1 mm→chars mapping + pan/zoom model <!-- agent: layout-designer-engineer.build, depends_on: [2.3, 3.1], touches: [src/physical.rs, src/ui.rs, src/app.rs] -->
      · aspect-compensated mapping (D4: rows/mm ≈ 2 × cols/mm) over the whole rack (row offsets + fold-bar heights from RackLayout); `physical_offset` + `physical_zoom` state on App; scale presets (`+`/`-`) map to zoom levels around a fixed anchor; wheel-pan when the rack overflows; component_rects published from the same formula (renderer-owns-geometry contract)
      · verify: `cargo test` mapping unit tests (round-trip mm→screen→mm, zoom anchor stability, pan offset math, multi-row offsets)
- [x] 4.2 Replace main panel view with physical render <!-- agent: layout-designer-engineer.build, depends_on: [4.1], touches: [src/ui.rs] -->
      · the main view renders components onto the grid cells (labels/state/shift/dim/pause, LED-folding + boxed cells preserved); the case outline, fold bars at row boundaries, and top/side mount regions render in both skeleton and full; multi-circuit instances render as side-by-side faceplates; over-long labels ellipsize; component_rects hit rects exactly match rendered cells
      · verify: `cargo test` + snapshot review: physical-layout frames replace the wrapped-panel frames; hit rects match rendered cells (no neighbor spill)
- [x] 4.3 Physical-view keys + status hints <!-- agent: rusty-engineer.build, depends_on: [4.2], touches: [src/handler.rs, src/app.rs] -->
      · pan keys, skeleton-toggle key, zoom via existing `+`/`-`; status hints (pan/zoom/skeleton state); no interference with prefix/viewer/graph/diff priority
      · verify: `cargo test` handler tests for the new keys; status hint text asserted
- [x] 4.4 `[physical]` + `[physical.rack]` config defaults <!-- agent: rusty-engineer.fast, depends_on: [4.2, 2.3], touches: [src/config.rs] -->
      · optional `[physical]` (default zoom, pan/scroll behavior) + `[physical.rack]` (rows: array of {he, hp, label?}, top_mount_te, side_mount_te, assign map) config parsed into a RackSpec via the existing injected-validation pattern; malformed rows → default case + warn-once
      · verify: `cargo test` config discovery/load/save/fallback tests pass
- [x] 4.5 Wire `[physical]` defaults into physical-view initialization <!-- agent: rusty-engineer.fast, depends_on: [4.4], touches: [src/app.rs, src/main.rs, src/ui.rs] -->
      · App gains `physical_rack_spec`; main.rs seeds scale_factor + physical_zoom from `zoom`, the pan origin from `offset_x`/`offset_y`, the presentation mode from `show_skeleton`, and the rack from `[physical.rack]` (mirroring the cost_model pattern); both physical renderers pack with `app.physical_rack_spec` (empty rows → default single-row case)
      · verify: `cargo test` green; absent `[physical]` leaves out-of-box view untouched

## 5. Verification & Docs

- [ ] 5.1 Coincidence proof tests <!-- agent: horst-engineer.build, depends_on: [3.3, 4.2], touches: [src/regression.rs, fixtures/**] -->
      · for each physical-layout fixture at fixed viewports: every full-view element rect equals its skeleton cell (same scale/offset); a fixture with two `[p2b8]` instances proves the faceplate path; overflow fixture proves pan consistency; a multi-row rack fixture (with fold bars + mount regions) proves row offsets and coincidence across rows
      · verify: `cargo test` (strict, incl. `cargo insta test --check`) green
- [ ] 5.2 Gallery skeleton | full rows <!-- agent: horst-engineer.build, depends_on: [5.1], touches: [src/bin/snapshot-gallery.rs] -->
      · the visual matrix renders physical-layout scenarios twice — skeleton + full — side by side (visual proof requirement)
      · verify: `cargo test -- --generate-gallery` produces the new rows; gallery HTML renders both frames
- [ ] 5.3 Update ARCHITECTURE.md/DESIGN.md + sync specs <!-- agent: devops-engineer.fast, depends_on: [4.4, 5.2], touches: [ARCHITECTURE.md, DESIGN.md, openspec/specs/**] -->
      · document the physical grid model, rack/case model (rows, TE mounts, row assignment), skeleton mode, 1:1 main view, and verification harness; sync the delta specs (controller-panels, module-scaling, keybinding, mouse-interaction, visual-validation) into `openspec/specs/`
      · verify: docs describe the new architecture accurately; `openspec validate` on the change passes
- [ ] 5.4 Full verification gate <!-- agent: horst-engineer.fast, depends_on: [5.3], touches: [] -->
      · `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test` (incl. `cargo insta test --check`), `cargo build --release --locked` — all four exit 0
      · verify: report the four gate results and any residual risk