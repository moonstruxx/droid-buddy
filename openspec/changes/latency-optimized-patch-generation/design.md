# Design: latency-optimized-patch-generation

## D1 — Pure optimizer core (`src/optimize.rs`)

Mirrors `layout.rs`/`latency.rs`: pure, deterministic, no terminal dependency.

Objective (min-sum): `Σ_edges ((t−s) mod N) × AVG(source)` — weighted circular Minimum Linear Arrangement, NP-hard in general, but N is small for musical patches and the wrap-around is cheap (`s=N−1 → t=0` costs 1 step). Feedback cycles are fine: exactly one edge wraps, cost 1 if adjacent.

```rust
pub struct CandidateOrdering {
    pub label: &'static str,          // "banner min-sum" | "global min-sum" | "min-max"
    pub order: Vec<usize>,            // section indices into the original Patch.sections
    pub before: LatencySummary,
    pub after: LatencySummary,
}

pub fn generate_candidates(
    patch: &Patch,
    cost: &CostModel,                 // D2
    scope: OptimizeScope,             // Banner (default) | Global
) -> Vec<CandidateOrdering>           // ≤ 3, best first
```

**Three variants** (all deterministic, bounded):
1. **Banner-preserving min-sum** (default scope) — permute sections within each banner group (and the implicit preamble group); group boundaries and group order fixed. Local search with a seeded starting permutation (topological-ish: producers before consumers where possible, seeded per-group).
2. **Global min-sum** — one permutation over all sections, same objective.
3. **Min-max (critical path)** — minimize the maximum per-edge latency instead of the sum; same scope as 1.

**Determinism**: seeds derive from node-id hashes (like `layout.rs`), no RNG; bounded iterations (`OPT_MAX_ITERATIONS`, ~2000 local-search steps with tabu-ish short memory); result quality verified against exhaustive search in tests for N ≤ 8 (all permutations enumerated, optimizer must match or beat).

## D2 — Shared configurable cost model

Extract the per-circuit `AVG` lookup from `latency.rs` into one provider used by **both** the coloring (`forward_latency`/`compute_latency`) and the optimizer, so a cost change recolors and re-optimizes coherently:

```rust
pub struct CostModel { overrides: HashMap<String, f32>, /* config [latency] per_circuit */ }
impl CostModel {
    pub fn circuit_avg(&self, node: &NodeId, schema: &Schema) -> f32;  // override ?? ramsize heuristic
    pub fn from_config(&config) -> CostModel;
}
```

- Default stays `AVG ∝ ramsize(circuit)` (today's heuristic).
- `config.toml` gains `[latency]`: `per_circuit = { "circuit-name" = <f32> }` (empty/absent = heuristic). Real values are a later user task; the shape is the deliverable.

## D3 — Constraints

- **Same-name relative order preserved**: instances of a repeated section name (`[button]` × 8) keep their mutual order in every candidate — instance numbers (DROID saved-state keys, manual 11.1) never reshuffle. Implemented as a projection: the search permutes *instance classes*, and within a class order is fixed.
- **Banner scope default**: for large/MPFS patches the banner groups are the natural stable units; default generation is banner-preserving. Global permutation is variant 2 and explicit.

## D4 — Lossless writer (`Patch::write_to_ini`)

The parser already records `raw_lines` + section `header_span`s. The writer block-slices by header line: every comment/banner line before a header belongs to that section and travels with it; the preamble (lines before the first header) stays first.

- Writing the original order must be **byte-identical** to the source file (round-trip test).
- The writer takes the *destination path* from the caller and refuses the canonicalized source path (save-as only).
- Atomic write (temp file + rename, same pattern as `LabelStore`), and auto-suffixes an existing destination (`-latopt.ini` → `-latopt-1.ini` …) rather than overwriting.

## D5 — UX (`g o` menu)

`g o` (works from any surface like `g d`; no patch → status hint) generates candidates and opens a centered overlay menu mirroring the validation-modal pattern:

- List up to 3 candidates: variant label + `avg X→Y · max A→B · back-edges N→M`.
- `j`/`k` navigate; `Enter` **previews** — reorders `Patch.sections` in memory, rebuilds graph + latency (`Event::GraphRebuilt`), so the existing ramp recolors live; status shows the candidate label.
- `s` exports the selected candidate via D4 to `<name>-latopt.ini` in the source directory; status confirms the written path.
- `r` restores the original order; `Esc` closes the menu (restoring the original order if a preview is active).

State: `App.optimizer: Option<OptimizerState { candidates: Vec<CandidateOrdering>, cursor: usize, previewing: Option<usize>, original_order: Vec<usize> }>`. Preview mutates section order in place; restore reverses it. Reordering only ever reorders `Patch.sections` — the writer's source of truth — so renderer, graph, and latency all follow automatically.

## D6 — Testing & snapshots

- Optimizer unit tests: brute-force equivalence (N ≤ 8), same-name order preserved, banner scope respected, determinism (two runs identical), min-max variant bounds, empty/single-section patches.
- Writer tests: byte-identical round-trip, comment/banner travel, reordered output valid (re-parses, same sections), refusal on source path, suffix-on-collision.
- Regression snapshots: menu modal (classic/terminal/mono), preview recoloring fixture, `cargo insta test --check` green.