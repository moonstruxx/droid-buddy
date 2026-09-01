# Tasks — DB8E OLED display placeholder (db8e-oled-display-placeholder)

## 1. Theme token for display placeholder

- [ ] 1.1 Add `display_placeholder` semantic token to `Theme::{classic,terminal,mono}` (no hardcoded `Color::` in renderer) <!-- agent: layout-designer-engineer, tier: 1, depends_on: [], touches: [src/theme.rs] -->
      · extend `Theme` with `display_placeholder: Color`, assign per-palette values (classic = muted neutral, terminal = Reset, mono = gray distinct from other grays), keep `active()`/`THEMES` wiring; pair with a token-coverage unit test.
      · verify: `cargo test theme` token coverage passes (all palettes distinct where required), no `Color::` literal in `src/ui.rs` for the placeholder.

## 2. Patch/model derivation of display state text

- [ ] 2.1 Add pure helper `db8e_display_state(patch|chain) -> &str` deriving placeholder text from patch presence (heuristic) <!-- agent: rusty-engineer, tier: 2, depends_on: [], touches: [src/patch.rs, src/physical.rs] -->
      · heuristic (YAGNI, no live circuit): no `[db8e]` / no `PhysicalLayout` module with `geometry_key == "db8e"` → `"not used"` (manual ch. 6.5.5 *not used by patch*); else if declared chain mismatches wired chain (stub: deterministic mismatch check, fallback to `"connected"` when ambiguous) → `"configuration error"`; else → `"connected"` baseline; pure function with module test.
      · verify: in-module unit tests cover the three states (no db8e, with db8e, stub mismatch) using `Patch::from_ini_str` fixtures; no parser schema change.

## 3. Bordered Display placeholder in DB8E upper band (shared rack-structure path)

- [ ] 3.1 Render bordered Display placeholder with centered state text in DB8E upper band through `render_rack_structure` (both skeleton and full) <!-- agent: layout-designer-engineer, tier: 2, depends_on: [1.1, 2.1], touches: [src/ui.rs, src/physical.rs] -->
      · in `render_rack_structure`'s per-module loop, when `geometry_key == "db8e"`, compute upper-band rect above the B-grid (`element_cells["B"].min(y_mm)` → 38 mm band top inside the 6 HP faceplate; `y` < B-grid top, `h` = B-grid top - faceplate top, inset for border); draw `Block::default().borders(Borders::ALL)` with `display_placeholder` token and centered state text (truncated/ellipsized via `truncate_with_ellipsis`); shared path so `render_physical_skeleton` and `render_physical_full` both show it, D5 coincidence preserved; placeholder is decorative (no `component_rects` entry).
      · verify: manual check at zoom 100% shows bordered rect above B-grid; skeleton vs full rects coincide (reuse `physical_skeleton_geometry`).

## 4. Snapshot/regression for DB8E gallery scenario

- [ ] 4.1 Pin DB8E placeholder face with insta snapshots for `ui_review/db8e` across themes (force-tracked) <!-- agent: horst-engineer, tier: 2, depends_on: [3.1], touches: [src/regression.rs, src/snapshots/droid_tui__regression__visual_ui_review_fronts_snapshot@ui_review_db8e_*.snap] -->
      · update `visual_ui_review_fronts_snapshot` to assert placeholder presence for fixture `fixtures/ui_review/db8e.ini` (gallery setup `src/gallery.rs:200`) at width 100 under `classic`/`terminal`/`mono` (ANSI + HTML faces); accept with `cargo insta review`; snapshots stay force-tracked (`.gitignore` + `!src/snapshots/**`).
      · verify: `cargo test visual_ui_review_fronts_snapshot -- --nocapture` green; insta pending snapshots cleared; `git status` shows snapshots as tracked.

## 5. Visual validation gallery check

- [ ] 5.1 Verify gallery row `ui_review_db8e` shows the placeholder and passes the coincidence gate <!-- agent: horst-engineer, tier: 1, depends_on: [4.1], touches: [src/gallery.rs] -->
      · run `cargo test -- --generate-gallery` or `cargo run -p droid_tui --bin snapshot-gallery` and inspect `evidence/gallery` HTML for row `ui_review_db8e · width 100 · DB8E front (6 HP, B1.1–B1.8 + E1.1)` — bordered Display rect with centered text visible in both skeleton/full contexts; confirm `physical_full_rects` vs `physical_skeleton_rects` coincidence still holds.
      · verify: gallery HTML renders the placeholder; no coincidence regression (`cargo test physical` green); document evidence path in this task's completion note.
