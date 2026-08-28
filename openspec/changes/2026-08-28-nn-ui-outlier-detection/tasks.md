# Tasks — 2026-08-28-nn-ui-outlier-detection

## 1. Data & Fitting (offline, tooling)

- [x] 1.1 Rebalance synthetic label pool + document label semantics <!-- agent: devops-engineer.build, depends_on: [], touches: [tools/build_features.py, corpus/features.csv] -->
      · BAD_POOL currently E/B-sourced only → 50 B-sourced outliers escape the 8.0 rule and any model overfits the generator (design D3)
      · add near-distance + cross-controller + non-E cases; keep `random.Random(SEED)` determinism; regenerate corpus/features.csv in the same run
      · verify: `python3 tools/build_features.py` regenerates the CSV (schema unchanged), and `git diff corpus/features.csv` shows label rebalancing, not schema drift
- [x] 1.2 Offline fit + evaluation script → distilled artifact <!-- agent: devops-engineer.build, depends_on: [1.1], touches: [tools/fit_outlier_model.py, tools/outlier_artifact.*] -->
      · stdlib-only `tools/fit_outlier_model.py`: fit bounded decision table (≤ few KB, design D1) on rebalanced features.csv; holdout precision/recall report vs the current 8.0 rule; emit the artifact file consumed by `include_str!`
      · gate: precision ≥ 0.60 at recall ≥ 0.86 on holdout; fallback row preserves the threshold rule (design D1)
      · verify: script exits 0 and prints the precision/recall report meeting the gate; artifact file exists with a stable byte content

## 2. In-process Scoring (runtime)

- [ ] 2.1 Embed artifact + pure Rust scorer in geometry/graph <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.2], touches: [src/geometry.rs, src/graph.rs] -->
      · `include_str!` the learned table (schema.rs embed precedent); score `BindingFeatures` → outlier Warning in the existing `TopologyIssue` channel
      · delegate the call site to the scorer with the invariant guards kept explicit (design D5): adjacent / co-located `L→B` / via-cable never flagged, miss → threshold fallback
      · verify: `cargo test` graph/geometry tests pass; new unit tests cover a scored-outlier case, a fallback case, and the invariant guards
- [ ] 2.2 Second-opinion layer: per-token influence_subtree z-score <!-- agent: dermannmitdermachine-engineer.build, depends_on: [2.1], touches: [src/patch.rs, src/graph.rs] -->
      · bake corpus per-token-kind mean/std of `influence_subtree` size into the artifact style (design D4); z-score beyond calibrated band → `TopologyIssue` Warning via the existing channel; never gates patch loading
      · verify: `cargo test` covers a flagged extreme-token case and a typical-token non-flag case (spec scenarios); patch still loads with the warning present

## 3. Regression & Proof

- [ ] 3.1 Regression + proof tests <!-- agent: horst-engineer.build, depends_on: [2.2], touches: [src/regression.rs, fixtures/**] -->
      · precision/recall regression vs 8.0 on holdout (tooling output asserted in a test); invariant tests (adjacent / co-located / via-cable never flagged, miss → fallback); snapshot fixtures for the new warning channel
      · verify: `cargo test` (strict, incl. `cargo insta test --check`) passes; snapshot fixtures render the new warnings in the graph surface