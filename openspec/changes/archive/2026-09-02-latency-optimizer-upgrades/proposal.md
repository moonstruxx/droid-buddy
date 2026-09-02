# Change: latency-optimizer-upgrades

## Why

The archived `latency-optimized-patch-generation` change shipped a deterministic optimizer: exact enumeration for N ≤ 8 plus a ~2000-step bounded local search with `safe_swap`. A follow-up survey (2026-08-28, user-reviewed) classified the objective — `Sum((t−s) mod N)×AVG(source)` over section orderings — as a weighted linear arrangement with circular wrap (MinLA/FAS relatives) and parked four upgrades that the current engine cannot express: it has no back-edge-targeting first phase, no coarsening path for large MPFS patches beyond the 2000-step budget, a single hard-coded objective with no average-vs-worst tradeoff, and no simulated-annealing strategy (previously excluded only because an unseeded RNG would break the D9 determinism contract). This change implements all four, sequenced FAS-indegree first.

## What Changes

- **FAS-indegree first phase** (`src/optimize.rs`): a cheap ranking pass over the edge set (FAS-indegree relatives) that orders sections to target back edges first, used to seed the existing bounded local search instead of the topological-depth seed. Deterministic, no RNG.
- **Multilevel coarsening + VNS** (`src/optimize.rs`): banner groups become coarsening hints — the search coarsens (contract) using group structure, solves the coarse problem, then refines via variable neighborhood search (VNS) with the existing `safe_swap` neighborhood; bounded work, deterministic.
- **Slider objective `(1−w)·Sum + w·max`** (`src/optimize.rs` + `g o` menu): the objective gains a user-adjustable weight `w ∈ [0,1]`; `w = 0` is pure min-sum, `w = 1` is pure min-max, and the *same* search engine evaluates the blended objective at any `w`. The optimizer menu gains keys to adjust `w` and re-generates candidates live.
- **SA with seeded PRNG** (`src/optimize.rs`): simulated annealing as an additional candidate strategy, using a seeded PRNG (node-id-hash seed) so results stay deterministic and reproducible per the D9 contract. Same-name relative order and banner-scope default preserved.
- **Constraints retained**: same-name instance order (saved-state mapping), banner-scope default, shared `CostModel`, and the `forward_latency` metric all unchanged; export remains save-as-only.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `latency-optimizer`: candidate-generation strategy set extended (FAS-indegree first phase, multilevel coarsening + VNS, SA with seeded PRNG); objective gains a weighted slider form; determinism and bounded-work requirements extended to the new strategies.

## Impact

- Affected specs: `latency-optimizer` (delta)
- Affected code: `src/optimize.rs`, `src/app.rs`, `src/handler.rs`, `src/ui.rs`, `src/theme.rs`, `src/regression.rs`, `fixtures/*`
- Baseline: full suite (528 tests) stays green; `cargo insta test --check` remains the strict gate.

## Non-goals

- No change to the latency metric (`forward_latency` unchanged) — the optimizer still minimizes what the graph colors
- No controller-instance remapping or saved-state rewriting
- No overwriting the source patch — export is save-as only
- No hardware integration
- No change to the cost-model config surface