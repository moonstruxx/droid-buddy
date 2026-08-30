## Why

The test suite has 16 pre-existing failures (`cargo test --lib`: 641 passed / 16 failed) that block the strict CI gate (`cargo insta test --check`, clippy `-D warnings`). The main view moved to the physical 1:1 rack renderer, but a cluster of tests still asserts the old wrapped-panel-era behavior, so the suite no longer reflects the shipped renderer; two genuine renderer edge cases (adjacent module borders overlapping by 1 column at mm→screen mapping, and switch values not rendering on the physical view) are also exposed by the failing tests.

## What Changes

- **Rewrite stale behavior tests** (~10 in `regression.rs`): wrapped-panel-era assertions (boxed LED cells, panel uniform rows, knob rendering with the embedded viewer, cell geometry/no-overflow, hit rects at non-default scale, mixed grids, modifier+shift coexistence, theme boxed cells) are updated to assert the physical-era contracts, mirroring the passing `physical_coincidence_*` tests.
- **Re-accept stale visual snapshots** (5 `visual_*` insta snapshots): `visual_boxed_vs_plain_led_pairs`, `visual_controller_panels_arpeggio`, `visual_multi_module_p2b8`, `visual_theming_shift_and_mono`, `visual_viewer_live_interaction` re-accepted to the physical-era faces (via `cargo insta accept --include-ignored`).
- **Reconcile corpus/tooling tests** (2): `regression_scorer_holdout_agrees_with_corpus` and `rendermetrics::tests::python_rust_extractor_agreement_on_corpus` — corpus files and/or extractor tooling drift, re-synced so the agreement holds.
- **Fix renderer edge case: mm→screen border overlap** — adjacent module borders overlap by 1 column at the current mm→chars mapping factor (~0.15 cols/mm); adjust the mapping or cell rect math so borders abut without overlap.
- **Fix renderer edge case: switch values not rendering** — `S1.1` collapses onto the Pot faceplate with no switch cell; switch components render their value on the physical view like knobs/encoders (mirroring the panel view's switch value rendering).
- **Fix 4 clippy lints** (2 pre-existing in `config.rs`, 2 in `patch.rs`) so `cargo clippy --all-targets --all-features --locked -- -D warnings` passes.

## Capabilities

### New Capabilities
- none

### Modified Capabilities
- `physical-scale-model`: two requirement deltas — module borders must not overlap at mm→screen mapping (rounding), and switch components must render their value on the physical view.

## Impact

- `src/regression.rs` — stale test rewrites + snapshot re-acceptance (main impact).
- `src/rendermetrics.rs` + `tools/` corpus artifacts (`tools/outlier_artifact.txt`, `tools/influence_stats.txt`, `tools/build_rendermetrics.py`) — corpus/extractor reconciliation.
- `src/physical.rs` / `src/ui.rs` — mm→screen mapping border overlap fix; switch value rendering on the physical view.
- `src/config.rs` / `src/patch.rs` — clippy lint fixes (no behavior change).
- Snapshot files under `src/snapshots/` — re-accepted to physical-era faces (force-tracked per project rule; commit with `git add -f`).
- No new dependencies; no public API changes.

## Non-goals

- No new renderer features beyond the two edge-case fixes (no new component kinds, no switch cells on controllers that physically lack them).
- No architecture changes; no spec additions for wrapped-panel rendering (that view is superseded, its assertions are rewritten or deleted, not resurrected).
- No CI configuration changes; the gate set stays as-is.
- No end-to-end live-terminal tests (out of the existing testing-strategy scope).