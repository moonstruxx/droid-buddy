# Tasks

## 1. Test rewrites — physical-era contracts (horst-engineer)

- [ ] 1.1 Rewrite `regression_boxed_cell_renders_led_frame_text_cell_stays_plain`, `regression_click_on_boxed_cell_toggles_and_selects`, `regression_mixed_grid_cells_coexist_without_overlap` to assert physical-view cell rendering (boxed LED cells, click toggle/select, coexistence without overlap) via the shared `physical_coincidence_*` fixture style; verify: these 3 tests pass and the renderer code paths they exercise are the physical view's <!-- agent: horst-engineer.build, depends_on: [], touches: [src/regression.rs] -->
- [ ] 1.2 Rewrite `regression_cell_geometry_no_overflow_overlap`, `regression_hover_hit_rect_matches_rendered_cell_at_nondefault_scale`, `regression_p2b8_knobs_render_fully_with_embedded_viewer_open` to assert geometry/hit-rect invariants under the physical renderer (hit rects match rendered cells at 75–200% zoom, embedded viewer open); verify: 3 tests pass <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [src/regression.rs] -->
- [ ] 1.3 Rewrite `regression_p2b8_panel_uniform_rows`, `regression_modifier_shift_plus_modifier_coexist`, `regression_theme_boxed_cells_and_shift_surfaces` to physical-era equivalents (row uniformity via module sub-blocks, modifier+shift coexistence, theme boxed cells + shift surfaces); delete assertions whose intent is superseded by an existing physical-era test; verify: 3 tests pass and no deleted assertion lacks a superseding test <!-- agent: horst-engineer.build, depends_on: [1.2], touches: [src/regression.rs] -->
- [ ] 1.4 Run `cargo test --lib regression` and confirm all rewritten `regression_*` tests pass (expect 0 failures in the lane) <!-- agent: horst-engineer.build, depends_on: [1.3], touches: [src/regression.rs] -->

## 2. Snapshot re-acceptance (horst-engineer, depends_on 1)

- [ ] 2.1 Re-accept the 5 stale visual snapshots (`visual_boxed_vs_plain_led_pairs_snapshot`, `visual_controller_panels_arpeggio_snapshot`, `visual_multi_module_p2b8_snapshot`, `visual_theming_shift_and_mono_snapshot`, `visual_viewer_live_interaction_snapshot`) to physical-era faces via `cargo insta accept --include-ignored`; review each `.snap` diff is a physical-era face (not a rendering regression); verify: `cargo insta test --check` passes with no pending snapshots <!-- agent: horst-engineer.build, depends_on: [1.4, 4.2], touches: [src/snapshots/] -->

## 3. Corpus/tooling reconciliation (api-engineer, depends_on 1)

- [ ] 3.1 Diff the failing assertion in `regression_scorer_holdout_agrees_with_corpus` against `tools/outlier_artifact.txt` and `corpus/features.csv`; re-sync corpus or table only when drift is real (holdout gate catches genuine model drift); verify: the test passes and the diff/reason is recorded in the commit message <!-- agent: api-engineer.build, depends_on: [1.4], touches: [src/regression.rs, tools/outlier_artifact.txt, corpus/] -->
- [ ] 3.2 Diff the failing assertion in `rendermetrics::tests::python_rust_extractor_agreement_on_corpus` between the Rust extractor and `tools/build_rendermetrics.py`; re-sync the extractor or corpus; verify: the test passes <!-- agent: api-engineer.build, depends_on: [3.1], touches: [src/rendermetrics.rs, tools/, corpus/] -->

## 4. Renderer edge cases (rusty-engineer)

- [ ] 4.1 Fix mm→screen border overlap: change `ScreenMapping`/module rect computation to derive column spans from absolute mm positions (`round(mm0×f)..round(mm1×f)`) so adjacent module borders abut at every zoom preset; add a regression test in `physical.rs` (alongside the coincidence tests) asserting no two adjacent module rects overlap across 75/100/150/200%; verify: `cargo test --lib physical` (55 tests) plus the new test pass <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/physical.rs] -->
- [ ] 4.2 Fix switch value rendering on the physical view: place switch cells from controller geometry (same mechanism as knobs/encoders/buttons) and render switch state glyph; when geometry lacks a switch cell for the controller, omit the switch rather than mis-rendering on a knob cell; verify: `switch_value.ini` fixture renders switch cells with state and hit-tests <!-- agent: rusty-engineer.build, depends_on: [4.1], touches: [src/ui.rs, src/physical.rs, fixtures/] -->

## 5. Clippy lints (rusty-engineer)

- [ ] 5.1 Fix the 2 pre-existing clippy lints in `config.rs` and 2 in `patch.rs` with minimal behavior-preserving edits (no `#[allow]`); verify: `cargo clippy --all-targets --all-features --locked -- -D warnings` exits 0 <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/config.rs, src/patch.rs] -->

## 6. Full gate (horst-engineer, depends_on 2,3,4,5)

- [ ] 6.1 Run the complete verification gate and confirm all four exit 0: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test` (full suite, incl. insta `--check`), `cargo build --release --locked`; verify: `cargo insta test --check` reports no pending snapshots and the suite shows 0 failures <!-- agent: horst-engineer.build, depends_on: [2.1, 3.2, 4.2, 5.1], touches: [] -->