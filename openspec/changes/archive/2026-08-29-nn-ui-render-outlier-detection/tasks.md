# Tasks — 2026-08-29-nn-ui-render-outlier-detection

## 1. Merge & Extractor

- [x] 1.1 Merge learned Track-2 scoring onto main <!-- merge landed as commit 22f135b (HEAD); verify via full gate 4.2 --> <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/geometry.rs, src/graph.rs, src/patch.rs, src/regression.rs, src/theme.rs, tools/build_features.py, tools/influence_stats.txt, corpus/features.csv] -->
      · merge `feature/2026-08-28-nn-ui-outlier-detection` onto main; resolve conflicts (geometry/graph/patch/regression/theme/tools) so the learned `WiringOutlierScorer` + `InfluenceStats` z-score second opinion and their proof tests exist on main
      · verify: `cargo test` (incl. `cargo insta test --check`) green; the merged tests from the branch pass; no duplicate or dead code left by the merge
- [x] 1.2 Pure render-metrics extractor <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/rendermetrics.rs] -->
      · `src/rendermetrics.rs`: pure `(Patch, width, theme) → RenderFeatures` (components/panels/modules counts, min_width, overflow_cols, fallback_rate, sidebar_hidden, minimap_hidden, min_contrast) derived from the renderer's own layout constants — no rendered frame, deterministic
      · verify: `cargo test` new in-module unit tests cover known fixtures at known widths (80/100/120 × classic/mono/terminal) and determinism (identical output on repeat)

## 2. Offline Fit & Embedding

- [x] 2.1 Offline corpus build + fit/distill → render artifact <!-- agent: devops-engineer.build, depends_on: [1.2], touches: [tools/build_rendermetrics.py, tools/fit_render_model.py, corpus/rendermetrics.csv, tools/render_artifact.*] -->
      · stdlib-only Python replica of the extractor (D4) generating `corpus/rendermetrics.csv` (corpus × widths 80/100/120 × themes, known-good + injected known-bad, `random.Random(42)` determinism); `tools/fit_render_model.py` fits the bounded decision table (≤ few KB, design D1) with holdout precision/recall vs the heuristic baseline; emits the artifact consumed by `include_str!`
      · gate: precision ≥ 0.60 at recall ≥ 0.86 on holdout; fallback row preserves the baseline rule (D5); schema stable vs the Rust side
      · verify: scripts exit 0 and print the precision/recall report meeting the gate; artifact file exists with stable byte content; `git diff corpus/rendermetrics.csv` shows label balancing, not schema drift
- [x] 2.2 Embed artifact + pure Rust scorer <!-- agent: dermannmitdermachine-engineer.build, depends_on: [2.1, 1.1], touches: [src/rendermetrics.rs] -->
      · `include_str!` the learned table; `score_render(features) → Option<RenderOutlier>` with invariant guards explicit (native-fit never flagged, baseline-clean never flagged, miss → heuristic fallback, D5); schema-drift check between the embedded table and the extractor
      · verify: `cargo test` covers a scored-outlier case, a fallback case, and both invariant guards; Python↔Rust extractor agreement test on a sampled corpus

## 3. Surfacing & CI

- [x] 3.1 Runtime surfacing: status_hint + theme token <!-- agent: layout-designer-engineer.build, depends_on: [2.2], touches: [src/app.rs, src/handler.rs, src/ui.rs, src/theme.rs] -->
      · score on `load_patch` at the current size/theme; degraded → status hint `Renders degraded at N cols — use ≥ M cols or reduce scale` in a new `render_outlier_warning` token (classic/mono/terminal); never gates loading, never intercepts input; ui.rs publishes the recommendation per frame
      · verify: `cargo test` status/theme tests; token present in all three palettes; snapshot of the status-hint channel renders in classic/mono/terminal at widths 80/100/120
- [x] 3.2 Gallery-CI render-outlier flag <!-- agent: devops-engineer.build, depends_on: [3.1], touches: [src/bin/snapshot-gallery.rs, .github/workflows/ci.yml] -->
      · the visual matrix marks scenarios the scorer predicts degraded; CI surfaces the flags as part of the existing gallery step
      · verify: gallery output marks predicted-bad scenarios; `cargo test -- --generate-gallery` + CI workflow lint pass

## 4. Regression & Gate

- [x] 4.1 Regression + proof tests <!-- agent: horst-engineer.build, depends_on: [2.2, 3.1], touches: [src/regression.rs, fixtures/**] -->
      · holdout precision/recall regression vs the heuristic baseline (tooling output asserted in a test); invariant matrix (native-fit never flagged, baseline-clean never flagged, miss → fallback); snapshot fixtures for the new status-hint channel; gallery scenario flagged in the matrix
      · verify: `cargo test` (strict, incl. `cargo insta test --check`) passes; snapshot fixtures render the new warning channel
- [x] 4.2 Full verification gate <!-- agent: horst-engineer.fast, depends_on: [3.2, 4.1], touches: [] -->
      · `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test` (incl. `cargo insta test --check`), `cargo build --release --locked` — all four exit 0
      · verify: report the four gate results and any residual risk