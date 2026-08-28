# latency-optimizer Specification

## Purpose

Generates latency-optimized candidate section orderings for a DROID patch, letting the user preview and save-as export them, using the same forward-loop latency metric the graph's latency coloring visualizes.

## Requirements

### Requirement: Candidate generation (up to 3)

The application MUST generate up to **3 candidate section orderings** for the loaded patch that minimize the forward-loop latency objective `Σ_edges ((t−s) mod N) × AVG(source)` (the same quantity the latency coloring visualizes). Generation MUST be deterministic (seeded from node-id hashes, no RNG) and bounded in work.

- Variants (best first): (1) banner-preserving min-sum, (2) global min-sum, (3) min-max (minimize the maximum per-edge latency).
- Each candidate MUST carry a label, the permutation, and before/after `LatencySummary { avg, max, back_edge_count }`.
- The objective and summaries MUST come from the shared latency model (`latency.rs::forward_latency`) unchanged.

#### Scenario: Deterministic generation

Given the same patch, two generation runs return identical candidate orderings and summaries.

#### Scenario: Brute-force equivalence

Given a patch with N ≤ 8 sections, each candidate's objective is no worse than the optimum found by exhaustive enumeration of all permutations under the same constraints.

#### Scenario: Min-max bounds the worst edge

Given a patch with one very long chain, the min-max variant reports a `max` no greater than the min-sum variant's `max`.

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