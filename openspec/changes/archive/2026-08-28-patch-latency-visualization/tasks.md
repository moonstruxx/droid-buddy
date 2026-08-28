# Tasks: patch-latency-visualization

## Task 1.1 — Pure latency model

- [x] done

- **agent**: dermannmitdermachine-engineer.build
- **depends_on**: —
- **touches**: `src/latency.rs` (new), `src/lib.rs`
- **details**: Create pure `src/latency.rs`: `EdgeLatency { edge_index, latency, is_back_edge }`, `LatencySummary { avg, max, back_edge_count }`, `forward_latency(edges, node_positions, circuit_avg) -> (Vec<EdgeLatency>, LatencySummary)` using `L(S→T) = ((t − s) mod N) × AVG` with `AVG(circuit) ∝ ramsize(circuit)` from the schema, normalized to a 0..1 axis. No terminal dependency; deterministic (no HashMap iteration). Wire `pub mod latency;` into `lib.rs`. Unit tests: forward chain (all low), back-edge (red end), empty graph, determinism (two runs identical).

## Task 1.2 — Graph integration

- [x] done

- **agent**: rusty-engineer.build
- **depends_on**: 1.1
- **touches**: `src/graph.rs`, `src/latency.rs`
- **details**: Compute latency as a build step in `Graph::build_from_patch` (like `validate_topology`): `Graph.latency: Option<LatencyData> { edges: Vec<EdgeLatency>, summary: LatencySummary }`, parallel to `graph.edges` by index. Expose the summary for the status surface. Keep it a pure addition — no rendering, no theme changes. Tests: latency data populated after build, parallel to edges, summary counts correct.

## Task 2.1 — Theme ramp + graph cable coloring

- [x] done

- **agent**: layout-designer-engineer.build
- **depends_on**: 1.2
- **touches**: `src/theme.rs`, `src/ui.rs`, `src/app.rs`, `src/handler.rs`
- **details**: Add `graph_edge_latency` ramp tokens (blue→red, ~5 stops) + `graph_edge_latency_legend` per theme (classic/terminal/mono, mono grayscale pairwise-distinct). Color cables in `render_graph` by `ramp[round(L/(N×AVG)×(stops−1))]` for non-error cables (error precedence unchanged). Legend/status line `latency avg X / max Y (1 loop ≈ 190µs)` + back-edge count; hover on a back-edge sink shows `reads _X 1 loop behind`. Add `App.latency_coloring: bool` (default true) + toggle key on the graph surface.

## Task 2.2 — Regression snapshot matrix

- [x] done

- **agent**: horst-engineer.build
- **depends_on**: 1.2
- **touches**: `src/regression.rs`, `fixtures/` (new latency fixtures)
- **details**: Add latency fixtures: (a) all-forward low-latency chain, (b) mixed mid-latency fan-out, (c) back-edge → red end, (d) topology-error cable coexisting with latency (error precedence). Snapshot matrix × classic/terminal/mono asserting stable cable colors; `cargo insta test --check` stays the gate. Full suite stays green (458 baseline + new).