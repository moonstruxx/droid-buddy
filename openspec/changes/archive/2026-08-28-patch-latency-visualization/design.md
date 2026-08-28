# Design: patch-latency-visualization

## D1 — Pure latency model (`src/latency.rs`)

A new pure module mirroring `validate_topology`/`diff.rs`/`geometry.rs`: no terminal dependency, deterministic, unit-testable.

```rust
pub struct EdgeLatency {
    pub edge_index: usize,        // parallel to Graph.edges
    pub latency: f32,             // L(S→T) = ((t − s) mod N) × AVG, in loop units
    pub is_back_edge: bool,       // s > t
}

pub struct LatencySummary {
    pub avg: f32,
    pub max: f32,
    pub back_edge_count: usize,
}

pub fn forward_latency(
    edges: &[GraphEdge],
    node_positions: &[usize],     // section_index per node
    circuit_avg: impl Fn(&NodeId) -> f32,
) -> (Vec<EdgeLatency>, LatencySummary)
```

- **Positions**: `GraphNode.section_index` (already in the model) — file order == processing order.
- **AVG derivation**: per-circuit `AVG ∝ ramsize(circuit)` from the schema; normalized so `Σ_edges L / max` lands on a stable 0..1 axis (`L/(N×AVG)`), keeping the *relative* gradient honest without claiming exact µs truth. The 190µs loop constant is used only for the legend ("1 loop ≈ 190µs").
- **Determinism**: pure function of the edge list + positions + ramsize map — no RNG, no HashMap iteration order (iterate `edges` by index).

## D2 — Graph integration (`src/graph.rs`)

`Graph` gains a `latency: Option<LatencyData>` computed after `build_from_patch` (like `validate_topology` runs as a build step). `LatencyData { edges: Vec<EdgeLatency>, summary: LatencySummary }`. Compute is triggered by the same call sites that build the graph (`open_graph`, `rebuild_graph`). Per-edge latency parallels `graph.edges` by index — no `HashMap` ordering hazard.

## D3 — Theme ramp (`src/theme.rs`)

New token group `graph_edge_latency`: a **ramp** of N stops (e.g. 5) from blue → red, plus a distinct `graph_edge_latency_legend` token.

- **classic**: blue `#1` → cyan → yellow → magenta → red (ANSI-16 legible, ramp endpoints distinct).
- **terminal**: all `Reset` (pairwise-identical is acceptable here — terminal theme is the "no color" baseline; snapshot still stable).
- **mono**: grayscale ramp — `White → Gray → DarkGray → (ramp stays grayscale)`; back-edge lands on the darkest distinct shade; must stay pairwise-distinct from `graph_edge_error` (red in classic, black in mono per existing token map).

**Precedence** (unchanged rule): `graph_edge_error` (red) > latency ramp > modifier hue > `CableKind`. A cable with a topology finding keeps error styling; latency coloring applies only to non-error cables.

## D4 — Rendering (`src/ui.rs` render_graph)

Cable color = `ramp[round(L/(N×AVG) × (stops−1))]`. Back-edges naturally map to the red end via the modulo. Legend line appended to the graph status: `latency avg X / max Y (1 loop ≈ 190µs)`, plus a back-edge count. Hover: when `hovered_graph_node` is a sink of a back-edge, the status line shows `reads _X 1 loop behind`.

## D5 — Toggle + state (`src/app.rs` / `src/handler.rs`)

`App.latency_coloring: bool` (default `true`), toggled by a key (proposal: `l` is picker, so `L`/`Shift+L` on the graph surface, or a graph-only key mirroring `x`/`p`; final binding chosen by layout-designer in 2.1). Toggle re-renders immediately; no graph rebuild needed (latency data is static per build).

## D6 — Regression coverage (`src/regression.rs` + `fixtures/`)

New latency fixtures exercising: (a) all-forward low-latency chain, (b) mixed mid-latency fan-out, (c) a back-edge (source after sink) → red end, (d) a topology-error cable coexisting with latency colors (error precedence). Snapshot matrix × classic/terminal/mono, asserting stable cable colors; `cargo insta test --check` stays the gate.

## Non-goals (deferred)

- **Module-order optimization / reordering** — follow-on change (C3); the model here is exactly the input that change's optimizer will minimize.
- **Write-back / save-as** — follow-on.
- **Exact µs truth** — AVG is a labeled ramsize-proportional estimate for relative coloring.