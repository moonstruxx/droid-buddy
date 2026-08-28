# Design: latency-optimizer-upgrades

## Context

The optimizer core lives in `src/optimize.rs`: `Objective` (MinSum/MinMax), `Strategy` (BannerPreserve/Global/MinMax), `search_exact` (enumeration, `ENUM_LIMIT = 50_000` permutations), `search_local` (~`SEARCH_STEPS = 2_000` safe_swap refinements seeded by `seed_order` from topological depth), and `generate_candidates` (entry point, returns up to 3 `CandidateOrdering`). The `g o` menu (`App.optimizer: Option<OptimizerState>`) lists candidates; `optimizer_preview` reorders `Patch.sections` in memory and `Event::GraphRebuilt` recolors. Motivation and scope: see proposal.md — Why.

The survey classified the objective as a weighted linear arrangement with circular wrap (MinLA/FAS relatives, not classical TSP). All new work must keep the D9 determinism contract (seeded from node-id hashes, no RNG), the bounded-work budget (`SEARCH_STEPS`), same-name relative-order preservation (`safe_swap`'s invariant), and banner-scope default.

## Goals / Non-Goals

**Goals**
- Four search-side upgrades shipped as layered phases in `src/optimize.rs`: FAS-indegree first-phase seed, multilevel coarsening + VNS, a weighted slider objective `(1−w)·Sum + w·max`, and SA with a seeded PRNG.
- One shared objective evaluator parameterized by `w`; every strategy feeds the same `generate_candidates` → `CandidateOrdering` pipeline so the menu, preview, and export paths stay unchanged.
- Deterministic + bounded for all strategies; same-name and banner-scope constraints hold for all.

**Non-Goals**
- No metric change (`forward_latency` stays the single source of truth for coloring and optimization).
- No change to `config.toml` cost-model surface or `CostModel` sharing.
- No change to export (save-as only) or preview/restore semantics.
- No multi-threading or async; the single-threaded event loop must never block perceptibly — hence bounded work for every strategy, including SA.

## Decisions

### D1: Phase ordering — FAS-indegree pass, then the existing engine

`generate_candidates` currently seeds each variant via `seed_order` (topological depth) and refines with `search_local`. Replace the seed for the local variants with a **FAS-indegree ranking pass**: compute per-section indegree over the edge set of the cable index (cable edges as in `graph.rs::build_from_patch`), then Kahn-style rank with a tie-break on node-id hash; place sections whose producers precede consumers, minimizing back edges before refinement. Rationale: it is the cheapest upgrade (one new function + seed swap), directly targets the `back_edge_count` component of the summaries, and the existing bounded local search already guarantees improvement-or-hold. Alternative considered: an exact FAS solver on the DAG after cycle removal — rejected as overkill; the ranking is only a seed, correctness is bounded by local search.

### D2: Slider objective as a parameterized evaluator, not a fourth variant

`Objective` gains a `Weighted(f32)` variant; `evaluate_order` computes `(1−w)·Sum + w·max` (Sum = forward-latency sum, max = max per-edge latency). `w = 0` and `w = 1` MUST reduce to the existing MinSum/MinMax paths (same comparator outcome), verified by test. The menu (`render_optimizer_modal`) gains a weight readout and `[`/`]`-style adjustment keys (see D5); changing `w` re-runs `generate_candidates` with the new objective and updates summaries. Rationale: `cmp_summaries`/`better` already abstract over `Objective`, so parameterizing the evaluator is the smallest change that reuses the entire search engine — no per-`w` engine. Alternative considered: a new `Strategy::Weighted` variant alongside the existing three — rejected, because the objective is orthogonal to the search strategy (a strategy is *how* to search, an objective is *what* to minimize); the menu's three rows keep their strategy identity while their summaries reflect `w`.

### D3: Multilevel coarsening + VNS keyed on banner groups

For domains above a threshold (reuse the `ENUM_LIMIT` check to detect "too big to enumerate"), the BannerPreserve strategy switches to multilevel: **coarsen** by contracting each banner group (and the implicit preamble group) into a single coarse section whose cost aggregates its members' edges (all intra-group edges become zero-cost in the coarse model — they cannot change, only inter-group edges matter at that level); **solve** the coarse problem with `search_local`; **refine** with VNS: uncoarsen and run successive safe_swap neighborhoods, shrinking neighborhood size after each no-improvement round, within the `SEARCH_STEPS` budget. Coarsening is a pure function of `Patch.sections` + `banner_groups` (already parsed), so it is deterministic. Rationale: banner groups are the natural coarsening hints the survey identified — they are the *only* movable units at the coarse level, which is exactly the banner-scope constraint, so the coarsening cannot violate it by construction. Alternative considered: random-walk multilevel (no group hints) — rejected; it ignores the structure the parser already provides and risks crossing group boundaries.

### D4: SA with a seeded PRNG (splitmix64) — determinism via construction

`Strategy` gains `Annealing`, available when the domain is small enough that the burn-in fits `SEARCH_STEPS` (fall back to `search_local` otherwise — the menu always shows 3 rows). The RNG is a **splitmix64** stream seeded from the same node-id-hash material as `seed_order` (fnv1a over sorted section token ids + banner index), so two runs on the same machine produce identical candidates. Temperature schedule: geometric cooling over `SEARCH_STEPS`, acceptance by Metropolis criterion on the objective delta (or the weighted objective per D2). Same-name order preservation is guaranteed because every proposed move is a `safe_swap` (the existing same-name-preserving operator). Rationale: the survey explicitly parked SA only because an *unseeded* RNG would break D9; a seeded, self-contained splitmix64 keeps the no-dependency, no-async constraints and is trivially testable (same seed → same output). Alternatives considered: `rand` crate with `StdRng` — rejected to avoid a new dependency for one seed; LCG — rejected for poor high-dimensional mixing; splitmix64 is a known-good 64-bit stream with zero dependencies.

### D5: Menu weight adjustment reuses the existing key surface

The `g o` menu already owns its keys while open (handler priority: picker → prefix → graph → viewer → optimizer menu is rendered before `render_main` when `app.optimizer.is_some()`; see `handler.rs` optimizer block). Weight adjustment uses `[`/`]` (matching the viewer-split convention of ±0.1 steps) to step `w` by 0.1 within `[0,1]`, `Esc` closes, `0`/`1` snap to the pure endpoints. Re-generation on weight change is synchronous and bounded (same `generate_candidates`), so the single-threaded loop stays responsive. Rationale: consistency with the existing split-ratio keys; a dedicated `w`-specific key would be undiscoverable. The status line reports `w = 0.4` alongside the candidate summaries.

## Risks / Trade-offs

- **SA budget pressure**: a bad temperature schedule could burn `SEARCH_STEPS` without improving over the local search. Mitigation: SA only runs when the domain is small enough to afford burn-in + refinements; otherwise the strategy falls back to `search_local`, and the menu still shows 3 rows (the label reflects which engine ran).
- **VNS threshold behavior**: choosing the coarsening threshold badly could route medium patches (e.g. 40–80 sections) to the slower multilevel path. Mitigation: keep `ENUM_LIMIT` as the switch — below it exact enumeration already wins; above it local search is already budget-starved, so coarsening is strictly better or equal.
- **Weighted-objective equivalence drift**: `w = 0`/`w = 1` must remain *semantically* the pure objectives (same comparator outcome). Guarded by a boundary test (see tasks 3.1); if the blended evaluator ever diverges from the pure paths, the pure paths remain authoritative for the summaries' `avg`/`max`.
- **Determinism across Rust versions**: splitmix64 is bit-exact and stable, but floating-point accumulation order in `evaluate_order` is fixed by the sorted edge iteration (already the case today); no new order-sensitivity is introduced.

## Open Questions

- Exact default for the coarsening threshold (propose: reuse `ENUM_LIMIT`; confirm during implementation if medium patches regress).
- Whether SA's fallback label should say "annealing (local)" or simply name the strategy — cosmetic, decided in UI task 2.2.