# 2026-08-28-nn-ui-outlier-detection

## Why

The hand-tuned `euclidean > 8.0 && cable_hops == 0` wiring-outlier rule floods: on the existing 4023-row labeled corpus it scores precision 0.15 at recall 0.86, because 96% of its false positives are cross-controller bindings the threshold cannot distinguish. The rule is provably the wrong decision boundary, and the labeled corpus now exists to learn a better one.

## What Changes

- Replace the fixed `WIRING_DISTANCE_THRESHOLD` check in `graph.rs` with a **compact learned decision artifact** (a few KB decision table) fitted offline from a rebalanced `corpus/features.csv`, embedded via `include_str!` and scored in-process by a pure Rust function — the `schema.rs` embed precedent, no new runtime dependencies.
- Rebalance the synthetic label pool in `tools/build_features.py`: today `BAD_POOL` is E/B-sourced only, so 50 B-sourced outliers escape the rule and any model overfits the generator. Add near-distance, cross-controller, and non-E cases while keeping the pipeline deterministic (seeded RNG).
- Add an offline fit/evaluation script (`tools/fit_outlier_model.py`) that reports holdout precision/recall against the current 8.0 rule and distills the artifact. Gate: precision ≥ 0.60 at recall ≥ 0.86 on holdout.
- Add a second-opinion layer: per-hw-token `influence_subtree` size z-score (corpus mean/std baked in), surfacing a Warning through the existing `TopologyIssue` / `graph_edge_error` channel.
- Preserve all existing invariants: co-located `L→B` and adjacent bindings are never flagged; via-cable bindings are never flagged; findings stay `TopologyIssue` warnings rendered with the error-highlight token.

## Capabilities

- **Modified Capabilities**:
  - `rack-wiring-outlier-detection` — the "Wiring-outlier topology warning" requirement changes from a fixed distance threshold to a learned decision artifact plus the per-token z-score second opinion.

## Impact

- **Code**: `src/graph.rs` (outlier decision delegates to the scorer), `src/geometry.rs` (scorer + embedded artifact, mirroring `schema.rs::include_str!`), `src/patch.rs` (per-token influence z-score statistics).
- **Tooling**: `tools/build_features.py` (rebalanced, deterministic label pool), new `tools/fit_outlier_model.py` + distilled artifact file.
- **Data**: `corpus/features.csv` regenerated (schema unchanged, rows/labels change).
- **Tests**: `src/regression.rs` + fixtures — precision/recall regression vs 8.0, invariant tests, snapshot fixtures for the new warning channel.
- **No** new runtime dependencies, no network, no async, no `.ini` mutation.

## Non-goals

- No linfa/ONNX runtime inference (documented future path only).
- No render-buffer outlier signal (roadmap signal 2) and no interaction telemetry (signal 3).
- No new runtime dependency of any kind.
- No change to the `TopologyIssue` / `graph_edge_error` surfacing channel.