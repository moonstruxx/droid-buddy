# latency-optimizer Specification

## Purpose

Generates latency-optimized candidate section orderings for a DROID patch, letting the user preview and save-as export them, using the same forward-loop latency metric the graph's latency coloring visualizes.

## Requirements

### Requirement: Candidate generation (up to 3)

The application MUST generate up to **3 candidate section orderings** for the loaded patch that minimize the forward-loop latency objective `Σ_edges ((t−s) mod N) × AVG(source)` (the same quantity the latency coloring visualizes). Generation MUST be deterministic (seeded from node-id hashes, no RNG) and bounded in work.

- Variants (best first): (1) banner-preserving min-sum, (2) global min-sum, (3) min-max (minimize the maximum per-edge latency).
- Every search strategy — including the extended set (FAS-indegree first phase, multilevel coarsening + VNS, simulated annealing) — MUST preserve the same-name relative-order and banner-scope constraints, MUST be deterministic across runs on the same machine, and MUST stay within a bounded work budget.
- Each candidate MUST carry a label, the permutation, and before/after `LatencySummary { avg, max, back_edge_count }`.
- The objective and summaries MUST come from the shared latency model (`latency.rs::forward_latency`) unchanged.

#### Scenario: Deterministic generation

Given the same patch, two generation runs return identical candidate orderings and summaries.

#### Scenario: Brute-force equivalence

Given a patch with N ≤ 8 sections, each candidate's objective is no worse than the optimum found by exhaustive enumeration of all permutations under the same constraints.

#### Scenario: Min-max bounds the worst edge

Given a patch with one very long chain, the min-max variant reports a `max` no greater than the min-sum variant's `max`.

### Requirement: FAS-indegree first-phase ranking

The search MUST begin with a cheap FAS-indegree ranking pass over the section orderings: sections are ranked to target back edges first (producers placed before their consumers), producing an initial ordering that the existing bounded local search then refines. The pass MUST be deterministic (derived from the patch's edge set and node-id hashes, no RNG) and MUST run before candidate search for every variant.

#### Scenario: Back edges targeted first

Given a patch containing a long circular dependency chain, the first-phase ranking orders the chain's sections so that back edges are minimized before local search begins; the final candidate's `back_edge_count` is no worse than a candidate generated without the first-phase pass.

#### Scenario: First phase is bounded

Given any patch, the ranking pass completes in time linear in the number of sections plus edges, regardless of patch size.

### Requirement: Multilevel coarsening + VNS

For patches whose section count exceeds the exact-enumeration bound, the generator MUST scale via multilevel search: banner groups (and the implicit preamble group) are used as coarsening hints to contract the problem, a coarse solution is computed, and it is refined by variable neighborhood search (VNS) over the same same-name-preserving pairwise-swap neighborhood used by the local search. Coarsening and refinement MUST be deterministic and MUST remain within the bounded work budget.

#### Scenario: Large patch convergence

Given a patch with many banner groups (e.g. 379-section MPFS library file), generation via coarsening + VNS completes within the bounded budget and returns a candidate no worse than the local-search-only candidate for the same objective.

#### Scenario: Coarsening preserves banner scope

Given any patch, coarsening never moves sections across banner-group boundaries; group boundaries and group order stay fixed, matching the banner-scoped default.

### Requirement: Weighted slider objective

The objective MUST support a user-adjustable weight `w ∈ [0,1]` blending average and worst-edge latency: the engine minimizes `(1−w)·Sum + w·max` over the edges of the section ordering, where `Sum` is the forward-loop latency sum `Σ_edges ((t−s) mod N) × AVG(source)` and `max` is the maximum per-edge latency. `w = 0` MUST reproduce the pure min-sum objective and `w = 1` the pure min-max objective. The same search engine MUST evaluate the blended objective at any `w`; no separate engine per `w`.

The optimizer menu MUST expose the current `w` and allow the user to adjust it, re-generating candidates with the new weight. Candidate labels and `LatencySummary` MUST reflect the weight in effect.

#### Scenario: Weight boundaries match existing variants

Given a patch, generation at `w = 0` yields candidates whose orderings match the pure min-sum variant, and generation at `w = 1` yields candidates whose `max` matches the min-max variant.

#### Scenario: Slider re-generates

Given a patch and a changed weight `w`, re-running generation with the new weight returns candidates evaluated with `(1−w)·Sum + w·max`, and the menu shows the updated weight alongside the candidate summaries.

### Requirement: SA with seeded PRNG

The generator MUST provide simulated annealing (SA) as an additional candidate strategy. SA MUST use a seeded PRNG (seeded from node-id hashes) so that results are deterministic across runs on the same machine, satisfying the D9 determinism contract, and MUST preserve the same-name relative-order and banner-scope constraints. SA runs MUST stay within the bounded work budget.

#### Scenario: SA deterministic

Given the same patch and seed, two SA runs return identical candidate orderings and summaries.

#### Scenario: SA respects constraints

Given a patch with repeated section names and banner groups, every SA-produced candidate keeps repeated instances in their original mutual order and never moves sections across banner-group boundaries.

### Requirement: Same-name relative-order preservation

Every candidate MUST preserve the relative order of circuit instances that share a section name (repeated `[button]`, `[knob]`, …). Instance numbers — the keys of DROID's saved-state mapping — must never reshuffle.

#### Scenario: Repeated section instances

Given a patch with 8 `[button]` sections, every candidate keeps the button instances in their original mutual order (only other sections may move relative to them).

### Requirement: Banner-scoped default

By default the generator MUST optimize within banner groups (and the implicit preamble group): sections may move inside their group, but group boundaries and group order are fixed. Global permutation is only offered as an explicit variant.

#### Scenario: Large MPFS patch

Given a patch with many banner groups (e.g. 379-section MPFS library file), the default generation permutes only inside each banner group and never moves sections across group boundaries.

### Requirement: Configurable per-circuit cost model

The per-circuit `AVG` used by both the latency coloring and the optimizer MUST be configurable through user config (`config.toml [latency]` per-circuit overrides). Circuits without an override fall back to the ramsize-derived heuristic. The coloring and the optimizer MUST share the same provider, so a cost change re-colors and re-optimizes coherently.

#### Scenario: Override changes optimization

Given a `[latency] per_circuit` override for a heavy circuit, generation with the override yields a different (or equal-better for the override objective) candidate set than without it.

#### Scenario: Shared with coloring

Given a cost override, the graph's latency ramp reflects the same per-circuit values the optimizer used.

### Requirement: In-memory preview

The application MUST be able to load a candidate ordering in memory: reorder `Patch.sections`, rebuild the graph and latency data, and emit `Event::GraphRebuilt` so the graph surface recolors immediately. The original order MUST be restorable without reloading the file.

#### Scenario: Preview recolors the graph

Given a candidate previewed, the graph rebuilds and cable colors follow the new order via the existing latency ramp.

#### Scenario: Restore original order

Given a preview active, restoring returns the patch to its original section order and the graph to its prior coloring.

### Requirement: Save-as export

Export of a candidate MUST write to a different filename than the source patch (e.g. `<name>-latopt.ini`) and MUST never overwrite the source. The write MUST be atomic and confirm the resulting path in the status line.

#### Scenario: Export writes a new file

Given a candidate selected for export, a new file appears next to the source with the reordered patch, and the source file is byte-identical to before.

#### Scenario: Collision auto-suffix

Given an existing `<name>-latopt.ini`, the export writes `<name>-latopt-1.ini` (next free suffix) rather than overwriting.