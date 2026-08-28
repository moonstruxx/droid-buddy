# Tasks: latency-optimized-patch-generation

## Task 1.1 — Shared configurable cost provider

- [x] Extract the per-circuit AVG lookup into a shared CostModel provider (used by latency coloring AND the optimizer) and add a `[latency]` config surface with per-circuit overrides in config.toml <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/latency.rs, src/config.rs] -->

**details**: `CostModel { overrides: HashMap<String,f32> }` + `circuit_avg(node, schema)` (override ?? ramsize heuristic); `from_config`; wire `graph.rs::compute_latency`/`latency.rs::forward_latency` to take the provider so a config change recolors AND re-optimizes coherently. `config.toml` gains `[latency] per_circuit = { "circuit" = <f32> }` (empty/absent = heuristic). Tests: override wins, absent falls back, config round-trip.

## Task 1.2 — Pure optimizer core

- [x] Create src/optimize.rs: up to 3 candidate orderings (banner-preserving min-sum default, global min-sum, min-max) minimizing the shared objective, with same-name relative-order preservation and banner-scope default; deterministic, bounded search; evaluate via latency.rs summaries <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/optimize.rs, src/lib.rs] -->

**details**: `CandidateOrdering { label, order: Vec<usize>, before: LatencySummary, after: LatencySummary }`, `generate_candidates(patch, cost, scope) -> Vec<CandidateOrdering>` (≤3, best first). Objective `Σ ((t−s) mod N)×AVG` via `forward_latency` (unchanged metric). Search: seeded permutation (node-id hashes, no RNG), bounded iterations (~2000 local-search steps). Wire `pub mod optimize;` into lib.rs. Tests: determinism, brute-force equivalence N ≤ 8, same-name order, banner scope, min-max bound.

## Task 1.3 — Lossless block-slicing writer

- [x] Add Patch::write_to_ini(dest): block-slice raw_lines by section header line (comments/banners travel with their section, preamble first), byte-identical round-trip, refuse the source's canonicalized path, atomic tmp→rename, auto-suffix on destination collision <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/patch.rs, src/lib.rs] -->

**details**: Writer over `Patch.sections` + `header_span`/`raw_lines`; accepts an arbitrary permutation. Wire into lib.rs. Tests: round-trip byte-identical, comment/banner travel, reordered output re-parses, source-path refusal, collision suffix.

## Task 2.1 — App state + `g o` menu + preview/export wiring

- [x] Add App.optimizer state (candidates, cursor, previewing, original_order) and handler wiring: `g o` generates + opens the menu, j/k navigate, Enter preview (reorder Patch.sections + rebuild graph + emit GraphRebuilt), r restore, s export via write_to_ini, Esc close (+restore if previewing); no-patch status hint <!-- agent: rusty-engineer.build, depends_on: [1.2, 1.3], touches: [src/app.rs, src/handler.rs] -->

**details**: `OptimizerState { candidates, cursor, previewing: Option<usize>, original_order: Vec<usize> }`; preview mutates section order in place, restore reverses; export refuses source path; status shows candidate label / written path.

## Task 2.2 — Candidate menu modal + status styling

- [x] Render the optimizer menu overlay (mirroring the validation-modal pattern), candidate summary lines with before/after values, theme tokens, and status surfacing; preview recoloring comes free from the existing latency ramp <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/ui.rs, src/theme.rs] -->

**details**: Centered overlay listing candidates (variant label + `avg X→Y · max A→B · back-edges N→M`), cursor highlight, hint line for j/k/Enter/s/r/Esc; token additions per theme (classic/terminal/mono).

## Task 3.1 — Optimizer + writer unit tests

- [x] Unit-test the optimizer (brute-force equivalence N ≤ 8, determinism, same-name relative order, banner scope, min-max bound, empty/single-section) and the writer (byte-identical round-trip, comment/banner travel, reorder re-parse, source-path refusal, collision suffix) <!-- agent: horst-engineer.build, depends_on: [1.2, 1.3], touches: [src/optimize.rs, src/patch.rs] -->

**details**: Brute-force cross-check enumerates all permutations for N ≤ 8 and asserts each candidate is no worse under the same constraints; full suite stays green (458 baseline + new).

## Task 3.2 — Regression snapshot matrix

- [x] Add fixtures + insta snapshots: optimizer menu modal (with candidates) and a preview-recoloring graph scenario, across classic/terminal/mono; `cargo insta test --check` stays the gate <!-- agent: horst-engineer.build, depends_on: [2.2], touches: [src/regression.rs, fixtures/] -->

**details**: New fixtures (reorderable patch with latency-relevant cables) and graph/menu snapshots; gallery scenario for the menu modal.