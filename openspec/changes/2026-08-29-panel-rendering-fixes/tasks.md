# Tasks: panel-rendering-fixes

## Task 1.1 — Narrow-width boxed-cell fallback (droid_tui-wsu)

- [x] In `src/ui.rs` boxed-cell rendering (`render_component` / `render_component_grid`): when the available cell width is smaller than the box content width, either shrink/truncate the content to fit inside a complete box or fall back to unboxed two-line rendering — never emit partial border fragments (stray ┌/┐ characters or glyphs landing on the border edge). Verify: regression test rendering an LED-associated component (e.g. Controller 3 B3.x cells from `fixtures/droid_mpfs5melody2.ini`) into a narrow-width `TestBackend` frame asserts no stray box-drawing fragments; `cargo insta test` snapshot at a narrow terminal width; `cargo test` green. <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/ui.rs, src/regression.rs, fixtures/] -->

**details**: Observed: entries like `○ [CLR/ S/E] ┐` and `[Track 1] S┐` show stray corner characters; in the Pot panel the trailing LED glyph lands on/outside the right border (`[3RD/SPREAD┐`). The renderer does not handle the case where the available width is smaller than the box content. Acceptance: no stray box-drawing fragments at any panel width; boxed cells either render complete boxes or fall back cleanly.

## Task 1.2 — Status bar segment dedup (droid_tui-rma)

- [x] In `src/ui.rs` `render_status`: find the code path that appends the Scale/Orientation (and any other) status segment twice and compose each segment exactly once. Verify: unit test asserting the composed status string contains no duplicated segment; `cargo insta test` snapshot of the corrected status bar; `cargo test` green. <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/ui.rs, src/regression.rs] -->

**details**: Observed: `Scale: 1.0 | Orientation: Landscape | Scale: 1.0 | Orientation: Landscape`. Acceptance: status bar shows each segment exactly once.

## Task 1.3 — Picker parent entry `..` (droid_tui-8zw)

- [ ] In `src/app.rs` `refresh_picker_entries` (≈:600): mark the parent-dir entry (index 0) with a sentinel so `render_picker` (src/ui.rs ≈:1070) displays `..` instead of the parent's `path.file_name()`. Enter on it must navigate up without closing (existing is_dir branch), no `..` entry when at filesystem root, and the `name == ".."` branch in `is_entry_selectable` (src/app.rs ≈:1741) becomes live (or is removed as dead code). Also sort entries dirs-first then `.ini` files (read_dir order is arbitrary). Verify: unit test for parent-entry labeling + Enter-up navigation + no `..` at root; `cargo insta test` snapshot of the picker showing the `..` entry; `cargo test` green. <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs, src/ui.rs, src/regression.rs] -->

**details**: Acceptance: picker shows `..` as first entry when not at root; Enter navigates to parent without closing; no `..` entry at root.

## Task 1.4 — Even vertical panel spacing (droid_tui-irf)

- [x] In `src/ui.rs` `render_patch_grouped` / `render_component_grid`: make the vertical rhythm between component rows consistent within a panel — boxed-vs-unboxed height differences and wrapping must not create extra blank rows (observed after B1.2, B1.5, B1.7 in the P2B8 panel). Verify: insta snapshot of the P2B8 panel from `fixtures/droid_mpfs5melody2.ini` showing uniform row spacing for same-kind cells; `cargo test` green. <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/ui.rs, src/regression.rs] -->

**details**: Acceptance: uniform row spacing within a panel for same-kind cells.

## Task 1.5 — Label ellipsis (droid_tui-lsd)

- [x] In `src/ui.rs`: add a truncation helper so over-long labels end with `…` (e.g. `[t2 P] Modulat…`) when the label exceeds the cell width, keeping hit rects and alignment unchanged. Verify: unit test for the truncation helper; `cargo insta test` snapshot showing ellipsized labels; `cargo test` green. <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/ui.rs, src/regression.rs] -->

**details**: Acceptance: over-long labels end with `…`.

## Task 1.6 — Minimum scale floor 75% (user request)

- [ ] In `src/handler.rs` (≈:912): change the scale preset cycle from `[0.5, 1.0, 1.5, 2.0]` to `[0.75, 1.0, 1.5, 2.0]` so users cannot scale cells below the boxable width (boxed cells need ~8 cols; 75% → 12 cols, comfortable). Wrap-around at both ends stays. Update `plus_and_minus_cycle_scale_presets_with_status` (src/handler.rs ≈:1542) — expectations 0.5 → 0.75, bottom wrap target 0.75. Verify: `cargo test` green (preset-cycle test updated); status shows `Scaling: 75%`. <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/handler.rs, src/regression.rs] -->

## Task 2.1 — Extended LED-association detection (droid_tui-8kr, part A)

- [ ] In `src/patch.rs`: audit the LED-association detection (bare `led = L.N` plus numbered `ledN = L.M` suffix-paired with `buttonN`/`potN`) and extend the suffix-pairing to ALL element param families in the schema that reference an LED per element — encoderN, switchN, faderN, and any other `ledN` groups revealed by `src/schema.rs` param expansion (read-only reference). Verify: unit tests asserting association resolution per kind (pot+LED, encoder+LED, switch+LED, fader+LED) on patch fixtures; existing tests stay green. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/patch.rs, fixtures/] -->

**details**: Depends on task 1.1 so more boxed cells do not amplify the narrow-width garbling. Acceptance: association detection covers bare `led =` plus suffix-paired `ledN` for all element param families in the schema.

## Task 2.2 — Joined-box rendering for all control kinds (droid_tui-8kr, part B)

- [ ] In `src/ui.rs` boxed rendering: render each control kind's state inside its joined box — knob/encoder percentage display, switch glyph, button state, fader — mirroring the LED-folded interior row; associated LEDs never render as standalone cells. Verify: insta snapshot / gallery scenario showing joined boxes for pot+LED, encoder+LED, switch+LED (fixture with such pairings); `cargo test` green. <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/ui.rs, src/regression.rs, fixtures/] -->

**details**: Acceptance: all control kinds with a resolvable LED association render as one boxed cell; associated LEDs never render standalone.