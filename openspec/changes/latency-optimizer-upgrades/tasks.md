# Tasks: latency-optimizer-upgrades

## Task 1.1 — FAS-indegree first-phase ranking

- [ ] Add FAS-indegree first-phase ranking (`fas_indegree_seed`): compute per-section indegree over the cable-index edge set, Kahn-style rank with node-id-hash tie-break, and use it as the seed for the local-search variants in place of the topological-depth seed. Verify: `cargo test optimize::` — new test asserts the ranking places producers before consumers on a circular chain (back-edge count no worse than `seed_order`) and that the pass is linear in sections+edges. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/optimize.rs] -->

**details**: Follows design D1. The cable-index edge set is the same the graph uses (cable sources → sinks, preamble maps excluded). `generate_candidates` currently seeds each local variant via `seed_order` (topological depth); replace that seed with the FAS-indegree rank (same signature: `fn(&[Range<usize>], &[IniSection]) -> Vec<usize>`). Deterministic: Kahn queue with node-id-hash (`fnv1a`) tie-break — no RNG. No change to `evaluate_order` or `search_local`.

## Task 1.2 — Multilevel coarsening + VNS

- [ ] Add multilevel coarsening + VNS (`coarsen_by_banner` / `search_vns`): contract each banner group (and the implicit preamble group) into one coarse section with zero-cost intra-group edges, solve the coarse problem with `search_local`, then uncoarsen and refine with shrinking safe_swap neighborhoods within `SEARCH_STEPS`. Verify: `cargo test optimize::` — VNS test asserts banner-scope preservation (no cross-boundary moves) and bounded convergence (≤ SEARCH_STEPS) on the large-patch fixture. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/optimize.rs] -->

**details**: Follows design D3. For BannerPreserve domains above the enumeration threshold (reuse the `ENUM_LIMIT` check), coarsening contracts each banner group to one coarse section — intra-group edges become zero-cost at the coarse level (they cannot change), only inter-group edges matter. Coarse solve via `search_local`, then uncoarsen + VNS: successive safe_swap neighborhoods, shrinking radius after no-improvement rounds, budget = `SEARCH_STEPS`. Coarsening is a pure function of `Patch.sections` + `banner_groups` — deterministic by construction, banner scope cannot be violated.

## Task 1.3 — SA strategy with seeded PRNG

- [ ] Add SA strategy with seeded splitmix64 PRNG (`Strategy::Annealing`): seed from node-id-hash material (same source as `seed_order`), geometric cooling over `SEARCH_STEPS`, Metropolis acceptance on the (possibly weighted) objective delta, every move a `safe_swap`; fall back to `search_local` when the domain is too large for burn-in. Verify: `cargo test optimize::` — same-seed determinism test (two runs, identical candidates) + same-name/banner-scope constraint test; fixture stays under ENUM_LIMIT so the fallback path is also unit-tested. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/optimize.rs] -->

**details**: Follows design D4. `Strategy` enum gains `Annealing`; the menu always shows 3 rows (label reflects which engine ran when SA falls back to `search_local` on domains too large for burn-in). splitmix64 is a self-contained 64-bit stream (no new dependency), seeded from fnv1a over sorted section token ids + banner index — same source as `seed_order`. Temperature: geometric cooling over `SEARCH_STEPS`; acceptance by Metropolis on the objective delta (works with the weighted objective from 1.4). Every move is a `safe_swap`, preserving same-name order.

## Task 1.4 — Weighted slider objective

- [ ] Parameterize the objective evaluator with a weight: add `Objective::Weighted(f32)`, make `evaluate_order` compute `(1−w)·Sum + w·max`, and keep `w = 0`/`w = 1` semantically identical to the existing MinSum/MinMax comparators. Verify: `cargo test optimize::` — boundary tests assert weighted(0.0) ≡ MinSum and weighted(1.0) ≡ MinMax ordering outcomes on a shared fixture. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.2, 1.3], touches: [src/optimize.rs] -->

**details**: Follows design D2. The objective is orthogonal to the search strategy: `Objective::Weighted(f32)` is a third objective the existing strategies (local, VNS, SA) all evaluate; the menu's three rows keep their strategy identity while their summaries reflect `w`. `cmp_summaries`/`better` already abstract over `Objective`; the weighted path reuses them. No new `Strategy` variant.

## Task 2.1 — Menu weight adjustment keys + App state

- [ ] Extend `App.optimizer` state with the current weight `w: f32` and re-generation on change: `[`/`]` step w by 0.1 in [0,1], `0`/`1` snap to endpoints, re-running `generate_candidates` with `Objective::Weighted(w)` and refreshing summaries. Verify: handler unit test drives `[`/`]`/`0`/`1` through the optimizer menu and asserts the candidate summaries change with w and clamp at the endpoints; `g o` still opens and Esc still closes. <!-- agent: rusty-engineer.build, depends_on: [1.4], touches: [src/app.rs, src/handler.rs] -->

**details**: Follows design D5. `OptimizerState` gains `weight: f32` (default 0.0); re-generation on change is synchronous and bounded (same `generate_candidates`) so the single-threaded loop stays responsive. `[`/`]` step ±0.1 (matching the viewer-split convention), `0`/`1` snap to pure endpoints. Status line reports `w = 0.4` alongside candidate summaries.

## Task 2.2 — Menu modal slider render + theme tokens

- [ ] Render the weight readout in the optimizer modal: show `w = 0.4` (and pure-endpoint labels at 0.0/1.0) in the menu header, using existing tokens or one new `optimizer_weight` token per palette, and show the weighted objective label on each candidate row. Verify: `cargo insta test --check` — new snapshot scenario renders the menu with w mid-range across classic/terminal/mono palettes; gallery fixture added. <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/ui.rs, src/theme.rs] -->

**details**: Follows design D5/D2. `render_optimizer_modal` (ui.rs) gains the weight readout in the header; candidate rows show the objective label (min-sum / min-max / weighted w) per the active objective. Theme: one `optimizer_weight` token per palette unless an existing token fits. Preview recoloring comes free from the existing latency ramp.

## Task 3.1 — Optimizer unit tests

- [ ] Unit tests for all four upgrades in `src/optimize.rs`: FAS-indegree ordering property; weighted-boundary equivalence (w=0≡MinSum, w=1≡MinMax); VNS banner-scope + bounded convergence; SA seeded determinism + constraint preservation; brute-force equivalence (N ≤ 8) for the weighted objective. Verify: `cargo test optimize::` passes; determinism test runs twice and compares. <!-- agent: horst-engineer.build, depends_on: [1.4], touches: [src/optimize.rs] -->

**details**: Mirrors the archived change's 3.1. Same-name relative order + banner scope assertions for each new strategy; determinism tests execute the strategy twice and compare candidate orderings + summaries.

## Task 3.2 — Regression snapshot matrix

- [ ] Regression snapshot matrix: extend `src/regression.rs` + `fixtures/` with an optimizer-menu scenario at two weights (w=0.4 and w=1.0) across all three themes, and a weighted-preview recoloring scenario on the graph. Verify: `cargo insta test --check` green (strict CI gate); `cargo run --bin snapshot-gallery` regenerates the gallery. <!-- agent: horst-engineer.build, depends_on: [2.2], touches: [src/regression.rs, fixtures/] -->

**details**: Mirrors the archived change's 3.2. Menu modal with weight readout + preview-recoloring graph scenario across classic/terminal/mono; fixtures for weighted objective at w=0.4 and w=1.0.