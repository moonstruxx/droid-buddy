# Tasks: all-claimable-beads-issues-solved

## Task 1 — Fix status bar after successful load (droid_tui-1uu)

- [x] Set `status_message` on both success paths of `load_patch` and `load_patch_at` (first-load-with-errors and clean). Add `regression` test rendering a loaded fixture and asserting status line lacks "No patch loaded". <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs, src/regression.rs] -->

**details**: `App::new` seeds `status_message` with "No patch loaded. Press 'l' to load."; the two `return true` paths never overwrite it. Set to `format!("Loaded {}", patch.name)` (fallback "Ready" if name empty) in all 4 success sites (2 functions × 2 paths), preserving the existing `ValidationCompleted` dispatch and modal logic. Test drives `App::load_patch(Patch::from_ini_str(... arpeggio1 ...))` through `render` `TestBackend` and asserts status row contains "Loaded" and not "No patch loaded". Snapshots that embed the status line will need `cargo insta accept --include-ignored`.

## Task 2 — Verify P8S8 fader column already ships (droid_tui-1oq)

- [x] Verify the vertical track + amber LED bar is already rendered for p8s8/m4 modules and close the issue with evidence (or file a follow-up if a gap is found). <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/ui.rs, src/physical.rs, src/theme.rs, evidence/gallery] -->

**details**: Check `physical_visuals` fader branch, `render_fader_track`, `module_is_fader`, `fader_led_bar` tokens (classic Yellow/terminal Reset/mono Gray distinct from led/knob), and `physical_multirow_rack.ini` snapshots. Generate gallery `cargo run --bin snapshot-gallery` and confirm fader rows show `▮` bar. No code change expected — the beads issue documents as verification.

## Task 3 — Verify adjacent-cell overlap is clamped (droid_tui-vj7)

- [x] Confirm D4 hit-rect clamping gives non-overlapping `component_rects` at all zoom presets and close the issue. <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/ui.rs, src/regression.rs] -->

**details**: The fix already exists in `render_physical_full` (`prev_right`/`prev_y` clamp). Run `cargo test adjacent_module_rects_never_overlap_across_zoom_presets` and the `regression_hover_hit_rect_matches_rendered_cell_at_nondefault_scale` path at zoom 1.5; both must pass. No code change expected.

## Task 4 — Document title-truncation decision (droid_tui-w2a)

- [x] Record that narrow-HP title truncation is intentional ("recognizability, not reproduction") with kitty-gfx as the labeled escape hatch. <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [DESIGN.md] -->

**details**: Add a one-paragraph note to `DESIGN.md` § Spacing/Physical or to the change archive. No code, no layout rework.
