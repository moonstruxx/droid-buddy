# Proposal — DB8E OLED display placeholder (db8e-oled-display-placeholder)

## Why

The DB8E controller (6 HP, B8+E) has a 128×64 pixel OLED display in its upper region. The physical 1:1 view currently renders the DB8E geometry as a bare faceplate with the B-grid starting at `y0_mm 38` and no display element cell (`controller_geometry.json` has no display cell; `element_cells` covers `B`/`E`/`L` only). The upper band above the B-grid (y < 38 mm inside the 128.5 mm 3 HE faceplate) is blank in both skeleton and full presentations, so the 1:1 view is not faithful to the hardware. Manual ch. 6.5.5 defines three display-owner states — *not connected*, *not used by patch*, *configuration error* — that the patch has no wired state for today.

This change adds a faithful placeholder for that display: a bordered rect in the DB8E upper band with centered state text, derived from the patch (no live hardware state yet). It makes the 1:1 view visually complete and pins the faceplate's upper-band layout for future live-display wiring (circuit `display`).

## What Changes

- **Theme token for the display placeholder** — new semantic token `display_placeholder` (classic/terminal/mono) so the OLED frame and its centered text restyle with the palette instead of a hardcoded color.
- **Patch/model-derived display state text** — pure derivation `db8e_display_state(patch) -> &str` with a conservative heuristic:
  - if the patch has no `[db8e]` section (no declared DB8E controller) → `"not used"` (manual: *not used by patch*);
  - else if the declared controller chain does not match the wired chain (stub: module-count/identity mismatch; full mismatch detection is a follow-up) → `"configuration error"`;
  - else → `"connected"` baseline (normal operation). The hook is kept in `patch.rs` / `physical.rs` so the later live `display` circuit state can replace the heuristic without touching the renderer.
- **Shared-path rendering in `render_rack_structure`** — both `render_physical_skeleton` and `render_physical_full` render the same bordered Display placeholder through `render_rack_structure` (the single rack-structure anchor at `src/ui.rs:630`), so skeleton and full coincide per D5. Per-module loop over `geom.module_rects`: when `geometry_key == "db8e"`, compute the upper-band rect above the B-grid (y0_mm 38) inside the faceplate, draw a bordered block (using the new token) with centered state text truncated/ellipsized to the rect width; no `component_rects` entry (decorative, not hit-testable).
- **Snapshot/regression gate for the DB8E gallery scenario** — extend `src/regression.rs` `visual_ui_review_fronts_snapshot` for `ui_review/db8e` (fixture `fixtures/ui_review/db8e.ini`, gallery setup `src/gallery.rs:200`) to pin the placeholder face under `classic`/`terminal`/`mono` at width 100; snapshots are force-tracked (insta `snapshots/**/*.snap` stays force-tracked despite `.gitignore`).
- **Visual validation gallery check** — gallery row `ui_review_db8e` continues to render the DB8E front at 100×50 under each theme; the placeholder is visible side-by-side in the gallery HTML and is the proof artifact for this change.

## Capabilities

### New Capabilities

- `db8e-display-placeholder`: upper-band OLED placeholder on DB8E faceplates (derived state text, bordered rect with centered text, theme-tokened).

### Modified Capabilities

- `physical-scale-model`: DB8E faceplate now carries a display placeholder in the upper band (above `y0_mm 38`); skeleton and full share the same structure path (`render_rack_structure`), preserving coincidence.
- `theming`: new `display_placeholder` semantic token (classic/terminal/mono) for the placeholder frame/text.
- `visual-validation`: `visual_ui_review_fronts_snapshot` pins the DB8E placeholder face across themes; gallery HTML is the visual proof.

## Impact

- `src/theme.rs` — new `display_placeholder` token in `Theme::{classic,terminal,mono}`; threaded through `active()`.
- `src/patch.rs` or `src/physical.rs` — pure helper `db8e_display_state(patch|chain) -> &str` implementing the heuristic; no parser schema change, no new token family.
- `src/ui.rs` — `render_rack_structure` gains the per-DB8E display-placeholder rect (upper band above `y0_mm 38`); called from both skeleton and full so the bordered rect + centered text is a single code path.
- `controller_geometry.json` — not modified (no new element cell; the placeholder is a derived upper-band rect, not a geometry family).
- `src/regression.rs` + `src/snapshots/**` — updated `ui_review_db8e` snapshots (ANSI + HTML) force-tracked.
- `src/gallery.rs:200` — unchanged logic, but the visual proof row `ui_review_db8e` now shows the placeholder.

## Non-goals

- No live `display` circuit wiring (register `D` / circuit `display` page 199) — the placeholder shows derived patch presence, not runtime pixel content.
- No 128×64 pixel fidelity or image/kitty rasterization of the OLED contents.
- No `controller_geometry.json` schema change or new element family (`D`) — the placeholder is a computed upper-band rect, not a per-element cell.
- No `L1.1..L1.8` vs full `L1..L32` LED-matrix change; LED rendering stays as today.
- No `[db8e]` configuration-error detection beyond a stub mismatch check; full hardware-vs-patch mismatch lives outside the viewer.
- No new persistence, config, or keybinding.

## Scope and Risks

**Scope:** DB8E module only (6 HP). Single rect in the upper band above the B-grid (`y0_mm 38`) inside the faceplate; centered text uses one of three states (`connected` / `not used` / `configuration error`). Renderer change is isolated to `render_rack_structure`'s per-module loop.

**Risks:**
- *Geometry drift*: `y0_mm 38` is the current B-grid top from `controller_geometry.json`; if geometry is regenerated, the constant must be re-derived. Mitigation: derive the placeholder's `y1` from `element_cells["B"].min(y_mm)` at runtime rather than a literal 38, with a literal fallback.
- *State heuristic overreach*: `"configuration error"` stub must not false-positive on valid patches with multiple controllers. Mitigation: stub returns `"connected"` unless an unambiguous mismatch (e.g. no DB8E wired but declared) is detected; the heuristic is documented as patch-presence only.
- *Snapshot churn*: theme token change touches `ui_review_db8e*` snapshots only; classic/terminal/mono snapshots regened via `cargo insta review`.
- *Coincidence*: placeholder renders through the shared `render_rack_structure` path so skeleton/full D5 coincidence holds (both call sites publish the same rack geometry).

## Alternatives Considered

- Adding a `D` family element cell to `controller_geometry.json` — rejected: the OLED is not an element cell (no `D` token row), and a fixed upper-band rect is simpler and avoids schema churn.
- Rendering the placeholder only in the full view — rejected: skeleton must show the structure's important visual characteristics (the display band is one); shared path keeps D5 coincidence trivial.

## Consumers

- `src/ui.rs:render_rack_structure` and its two callers (`render_physical_skeleton`, `render_physical_full`) are the direct consumers of the new token and state helper.
- Regression/gallery tests consume the rendered face; no downstream crate API change.
