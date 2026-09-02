# Change: all-claimable-beads-issues-solved

## Why

Four beads issues are `ready`/`in_progress` and block a clean `bd ready`:

- `droid_tui-1uu` (bug, P3): status bar still says "No patch loaded" after a successful `load_patch`. `App::new` defaults to that string and neither success path in `load_patch`/`load_patch_at` overwrites it. Misleading; snapshots show the bug.
- `droid_tui-1oq` (feature, P2): P8S8 fader column should render a vertical track + amber LED bar. `physical_visuals` + `render_fader_track` + `fader_led_bar` token + `module_is_fader(p8s8|m4)` already ship this — needs verification/closure, not new code unless gaps found.
- `droid_tui-vj7` (task, P3): adjacent physical cells can share a column at zoom 1.5 due to mm→screen rounding, overlapping `component_rects`. `render_physical_full` now clamps hit rects at draw time (prev_right/prev_y) — needs verification that the strict no-overlap invariant holds at all presets and closure.
- `droid_tui-w2a` (task, P3): module titles truncate to ~1 char at 0.15 cols/mm (5 HP ≈ 4 cols). Design intent is "recognizability, not reproduction" via geometry+glyphs, with kitty-gfx as escape hatch — needs an explicit decision and documentation, not a layout change.

A single focused change fixes the one live bug and reconciles the three observations.

## What Changes

- **Status bar bug** (`src/app.rs`): set `self.status_message = format!("Loaded {}", patch.name)` (or Ready) on both success paths of `load_patch` and `load_patch_at` (first-load-with-errors and clean). Add regression test rendering a loaded fixture and asserting the status line lacks "No patch loaded".
- **Fader column verification** (`src/ui.rs`/`src/theme.rs`/`src/physical.rs`): confirm `p8s8`/`m4` fader modules render via `physical_visuals` → `render_fader_track` with `fader_led_bar` token; snapshots `physical_multirow_rack` already cover it. No new code if glyph is correct — close with evidence.
- **Adjacent-cell overlap verification**: confirm D4 clamping in `render_physical_full` gives non-overlapping `component_rects` at zooms 0.75/1.0/1.5/2.0 for the multi-row fixture; existing regression `adjacent_module_rects_never_overlap_across_zoom_presets` is the gate.
- **Title-truncation decision**: record the design decision (truncation is intended; kitty-gfx remains the labeled escape hatch) in `DESIGN.md` or the change archive, no code.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `controller-panels`: status bar now reflects successful load (fix `droid_tui-1uu`).
- `physical-scale-model`: clarifies that title truncation at narrow HP is intentional (close `droid_tui-w2a`).

## Impact

- Affected specs: `controller-panels`, `physical-scale-model` (deltas)
- Affected code: `src/app.rs` (status_message), `src/regression.rs` (one new status-bar test), `src/snapshots/` (updated gallery/status snapshots if the fix changes the status line), `DESIGN.md` note for truncation
- Baseline: 4 beads issues move to closed; `bd ready` empty; gallery flag still green
- No new dependencies, no hardware bridge, no persistence change

## Non-goals

- No new glyph design for P8S8 (already shipped)
- No geometry or layout rework for title truncation
- No change to `load_patch` gating/error semantics beyond the status string
