# Proposal — 2026-08-29-nn-ui-render-outlier-detection

## Why

droid_tui renders deterministically, yet a patch can silently degrade at the user's terminal width and palette: boxed cells fall back to unboxed two-line rendering, panels clip, the source sidebar/minimap hide below their width thresholds, and the mono palette can fail contrast between co-occurring tokens. Today there is no channel that tells the user "this patch needs ≥ N columns" or that a gallery render is degraded — the degradation is only visible as a slightly worse screen.

This change is roadmap signal 2 (visual-render outliers). Its foundation is the learned-artifact pattern from signal 1's Track 2 (bounded decision table distilled offline, embedded via `include_str!`, scored by pure Rust) — but that Track-2 work (`WiringOutlierScorer` + `InfluenceStats` z-score) was implemented on `feature/2026-08-28-nn-ui-outlier-detection` and **never merged to main**: main only carries Track 1 (the hard 8.0 distance rule), and the archived-change note claiming it shipped does not match the tree. Restoring it is the first task of this change, so the pattern this change builds on actually exists on main.

## What Changes

- **Merge Track-2 learned scoring onto main:** merge `feature/2026-08-28-nn-ui-outlier-detection` (learned `WiringOutlierScorer` + `InfluenceStats` z-score second opinion + its corpus/tooling) into main with conflict resolution, restoring the learned-artifact channel and its proof tests.
- **Pure render-metrics extractor (`src/rendermetrics.rs`):** for each (patch, width, theme) compute deterministic layout features from the renderer's own constants — component/panel/module counts, minimum width needed, overflow columns, boxed→unboxed fallback rate, sidebar/minimap hidden flags, mono contrast minima between co-occurring tokens. No rendered frame required; pure math over the layout the renderer would produce.
- **Offline corpus + distillation (tooling):** expand the corpus × widths (80/100/120) × themes (classic/mono/terminal), label known-good and injected known-bad renders (narrow width forcing fallback, oversized patch at 80 cols), fit a compact decision table (≤ few KB) with a precision/recall gate vs a heuristic baseline, and emit an artifact consumed by `include_str!` — the Track-2 pattern.
- **Runtime scoring + surfacing:** score the loaded patch's render at the current terminal size/theme; on a degraded prediction, surface a status_hint (theme token) such as `Renders degraded at N cols — use ≥ M cols or reduce scale`. Never gates loading.
- **Gallery-CI render-outlier flag:** the visual matrix (snapshot-gallery + CI) flags scenarios predicted degraded, making the degradation a checked regression, not a silent one.

## Capabilities

### New Capabilities

- `render-outlier-detection`: deterministic render-geometry scoring — a pure extractor computing per-(patch,width,theme) layout features, an embedded distilled decision table with invariant guards (native-fit never flagged, miss → heuristic fallback), a status_hint surface for degraded renders, and a gallery-CI check flagging predicted-bad renders. Never gates patch loading.

### Modified Capabilities

- `visual-validation`: add the requirement that the gallery matrix flags scenarios the render-outlier scorer predicts degraded.
- `theming`: add a `render_outlier_warning` token (classic/mono/terminal) for the status_hint surface.

## Impact

- `src/rendermetrics.rs` (new) — pure extractor + embedded-artifact scorer.
- `src/app.rs` / `src/handler.rs` — score on `load_patch` at current size/theme; status_hint surface.
- `src/ui.rs` — publish render metrics per frame / render the warning in the status bar.
- `src/theme.rs` — `render_outlier_warning` token in all three palettes.
- `src/regression.rs` + `fixtures/**` — proof tests: holdout precision/recall regression, invariant matrix, snapshot matrix for the new channel.
- `tools/build_rendermetrics.py`, `tools/fit_render_model.py`, `tools/render_artifact.*` — offline corpus build + fit/distill (stdlib-only, deterministic SEED).
- `corpus/rendermetrics.csv` — regenerated feature set (schema-stable).
- `.github/workflows/ci.yml`, `src/bin/snapshot-gallery.rs` — gallery-CI render-outlier flag.
- Merge restore: `src/geometry.rs`, `src/graph.rs`, `src/patch.rs`, `src/theme.rs`, `tools/build_features.py`, `tools/influence_stats.txt`, `corpus/features.csv` from the Track-2 branch.

## Non-goals

- No ML runtime (ort/candle/linfa/ONNX) and no new binary dependencies — the distilled decision table is the whole runtime model (YAGNI).
- No telemetry, no network, no hardware bridge — unchanged from the architecture.
- The warning never gates patch loading and never blocks rendering.
- Roadmap signal 3 (interaction outliers) stays parked.
- No new keybindings — surfacing is status_hint only.