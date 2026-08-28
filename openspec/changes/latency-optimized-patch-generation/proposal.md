# Change: latency-optimized-patch-generation

## Why

DROID evaluates circuits **serially in file order once per ~190µs (~5.5 kHz) loop**. A circuit that reads a virtual `_cable` produced by a circuit running *later* in the same loop gets **last loop's value** — a full loop of lag the user probably did not intend. The latency-coloring change (`patch-latency-visualization`, archived) made this visible: every cable now carries a forward-loop latency `L(S→T) = ((t − s) mod N) × AVG` and is colored blue(low)→red(high).

Module order is a free parameter — DROID patches are ordered for *reading*, not for latency — so the obvious next step is to **generate section orderings that minimize the total forward-loop latency**. The objective the optimizer minimizes is exactly the quantity the graph already colors: one coherent metric, no new measurement.

Real DROID patches are small (typical musical patches 5–60 sections; MPFS library files up to ~379) and the loop is deterministic, so a bounded, deterministic search is tractable. One hard constraint shapes the solution: DROID's saved-state mapping is keyed by instance number (the order of same-name circuits, manual 11.1), so an optimizer must **preserve the relative order of same-name circuit instances** or it silently breaks saved controller state.

## What Changes

- New pure `src/optimize.rs`: generates up to **3 candidate section orderings** (variants: banner-preserving min-sum, global min-sum, min-max/critical-path), each evaluated against the objective `Σ_edges ((t−s) mod N) × AVG`. Deterministic (seeded from node-id hashes, no RNG), bounded iterations, brute-force cross-checked on small patches in tests. No terminal dependency.
- Shared **configurable cost model**: the per-circuit `AVG` lookup currently embedded in `latency.rs`/`graph.rs::compute_latency` is extracted into one provider that merges `config.toml [latency]` per-circuit overrides over the ramsize-derived default. The *same* provider feeds the latency coloring and the optimizer, so they stay coherent; real per-circuit values can be supplied later without touching rendering.
- New lossless **`.ini` writer** (`Patch::write_to_ini`): block-slices `Patch.raw_lines` by section header line so comments/banners travel with their section; writing the original order is byte-identical (round-trip lossless). **Refuses to write to the source patch's own path** — export only ever produces a differently-named file (e.g. `<name>-latopt.ini`, auto-suffixed if taken).
- **UX**: `g o` on the graph surface opens a candidate menu (up to 3 solutions, each with before/after `avg / max / back-edge count` summaries). `j`/`k` navigate, `Enter` loads the candidate **in memory** (preview: graph rebuilds and the existing latency ramp recolors immediately), `s` exports the selected candidate to the new file, `r` restores the original order, `Esc` closes the menu.
- **Constraints enforced by the generator**: relative order of same-name circuit instances preserved (saved-state mapping intact); **banner-scope is the default** (permute within banner groups, keep group order) — global permutation is offered only as an explicit variant.

## Impact

- Affected specs: new capabilities `patch-writing` + `latency-optimizer`; delta on `signal-flow-graph`
- Affected code: `src/optimize.rs` (new), `src/latency.rs`, `src/config.rs`, `src/patch.rs`, `src/app.rs`, `src/handler.rs`, `src/ui.rs`, `src/theme.rs`, `src/regression.rs`, `fixtures/*`
- Baseline: full suite (458 tests) stays green; `cargo insta test --check` remains the strict gate.

## Non-goals

- No overwriting the source patch — export is save-as only, always to a different filename
- No claim of exact per-circuit µs latency — default AVG stays a ramsize-proportional estimate; real values arrive later through the config surface
- No change to the latency *metric* (`forward_latency` unchanged) — the optimizer minimizes what the graph already colors
- No controller-instance remapping or state-file rewriting
- No hardware integration