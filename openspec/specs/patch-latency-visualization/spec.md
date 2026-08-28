# patch-latency-visualization Specification

## Purpose

Renders forward-loop signal latency on the graph surface: blue-to-red cable coloring by loop-step distance, a per-patch avg/max summary, and back-edge loop-delay hints, so users can see which cables lag a full DROID evaluation loop.

## Requirements

### Requirement: Forward-loop latency model

The application MUST compute, for the loaded `Patch`, a forward-loop latency for every signal-flow cable edge: `L(S→T) = ((t − s) mod N) × AVG`, where `s`/`t` are the file-order positions (`GraphNode.section_index`) of the edge's source and sink circuits, `N` is the module count, and a back-edge is `s > t` (the sink runs before the source in the loop and reads last loop's value, wrapping the loop boundary).

- Latency per circuit MUST be RAM-derived: `AVG(circuit) ∝ ramsize(circuit)`, scaled so the whole patch budget maps to the ~190µs (~5.5 kHz) loop, using the schema's `available_memory` and per-circuit `ramsize`.
- The model MUST be pure (no terminal dependency), in a new `src/latency.rs`, testable without rendering.
- The model MUST be deterministic: same patch → same latencies.

#### Scenario: Forward edge latency

Given a patch with modules ordered `A B C D` where `A` produces a cable consumed by `D` (positions 0 and 3, `N = 4`), the latency for that edge is `((3 − 0) mod 4) × AVG = 3 × AVG`, and `is_back_edge` is `false`.

#### Scenario: Back-edge wraps the loop

Given a patch with modules ordered `A B C D` where `D` produces a cable consumed by `B` (positions 3 and 1), the latency is `((1 − 3) mod 4) × AVG = 2 × AVG` (wrapping past the loop boundary), and `is_back_edge` is `true`.

#### Scenario: Deterministic output

Given the same patch parsed twice, `forward_latency` returns byte-identical edge latencies and summary on both runs.

### Requirement: Per-patch latency summary

The application MUST derive a per-patch summary from the model: average and maximum forward-loop latency across all edges, plus the count of back-edges. This summary MUST be surfaced in the status/legend line of the graph surface when latency coloring is active.

#### Scenario: Summary counts

Given a patch with 3 forward edges and 1 back-edge, the summary reports `avg` and `max` computed over all 4 edges and `back_edge_count = 1`.

#### Scenario: Summary in graph status

Given the graph surface open with latency coloring active, the status/legend line shows `latency avg X / max Y (1 loop ≈ 190µs)` using the patch summary values.

### Requirement: Blue-to-red cable latency coloring on the graph

The graph surface (`g g`) MUST color every cable by its forward-loop latency mapped onto a **blue(low)→red(high) ramp**, proportional to `L/(N×AVG)`. Back-edges MUST land at the red end of the ramp.

- The latency ramp MUST be a distinct set of theme tokens, separate from `graph_edge_error` (which remains reserved for topology errors) — a topology-error cable keeps its error styling regardless of latency color.
- The ramp MUST ship per theme (classic/terminal/mono) and remain pairwise-legible within each theme.
- Latency coloring MUST be toggleable (default on).

#### Scenario: Gradient mapping

Given edges with latencies at 0.1, 0.5, and 1.0 of the normalized axis, the cable colors map to the low, mid, and red-end stops of the ramp respectively.

#### Scenario: Back-edge lands at red end

Given a back-edge (source position > sink position), its cable renders at the red-end stop of the ramp.

#### Scenario: Topology-error precedence

Given a cable that carries a topology-validation finding (`graph_edge_error`), it renders with the error styling even when latency coloring is active; latency coloring applies only to non-error cables.

#### Scenario: Toggle off

Given `latency_coloring` toggled off on the graph surface, all cables render without the latency ramp (falling back to the existing `CableKind`/modifier/error precedence).

### Requirement: Legend with loop timing

When latency coloring is active, the graph surface MUST show a legend/status line: `latency avg X / max Y (1 loop ≈ 190µs)`. A hovered sink circuit MUST show `reads _X 1 loop behind` when the incoming cable is a back-edge.

#### Scenario: Hover on a back-edge sink

Given the mouse hovers a graph node that is the sink of a back-edge, the status line shows `reads _X 1 loop behind` for the cable `_X`.

#### Scenario: Hover on a forward-edge sink

Given the mouse hovers a graph node whose incoming cable is a forward edge, no `1 loop behind` suffix appears in the status line.

### Requirement: Deterministic snapshot coverage

The visual-validation regression harness MUST cover latency coloring: fixtures spanning forward edges (low/mid latency) and back-edges (red end), across classic/terminal/mono themes, asserting stable cable colors. `cargo insta test --check` remains the strict gate.

#### Scenario: Snapshot matrix

Given latency fixtures (all-forward chain, mixed fan-out, back-edge, error-coexistence) rendered across classic/terminal/mono themes, the snapshot harness produces stable ANSI/HTML output with the expected cable colors, and `cargo insta test --check` passes without pending snapshots.