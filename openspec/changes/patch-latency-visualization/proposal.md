# Change: patch-latency-visualization

## Why

DROID evaluates circuits **serially in file order once per ~190µs (~5.5 kHz) loop**. A circuit reads a virtual `_cable` from a circuit that runs *later* in the same loop and gets **last loop's value** — a signal the user probably did not intend. In the signal-flow graph view (`g g`) this is currently invisible: every cable renders identically, so a back-edge (source sorts after sink) that silently lags a full loop looks the same as a forward edge that is fresh in-loop.

The user wants the **age/latency relationship of every cable surfaced visually**: a continuous blue(low)→red(high) cable gradient proportional to forward-loop latency. This is a *visualization* capability built on the existing graph model (`graph.rs` already carries `GraphNode.section_index` for file order and `GraphEdge { source, sink }`); it introduces one small pure latency model and a color ramp. Reordering/optimization of module order is deliberately **out of scope** here and is a follow-on change.

## What Changes

- New pure `src/latency.rs`: `forward_latency` model — for each edge `S→T` with positions `s,t` (from `GraphNode.section_index[]`), the forward-loop latency `L(S→T) = ((t − s) mod N) × AVG`, where `N` = module count and `AVG` is a per-circuit latency derived from real data: `AVG(circuit) ∝ ramsize(circuit)` scaled to the loop budget (~190µs), from the schema's `available_memory` + per-circuit `ramsize`. A back-edge is `s > t` (wraps the loop boundary) and lands at the red end of the gradient. No terminal dependency.
- `src/graph.rs`: compute per-edge latency during/after `Graph::build_from_patch`; expose a `latency: Option<Vec<f32>>` (parallel to `graph.edges`) or fold onto `GraphEdge`; surface a per-patch avg/max summary.
- `src/theme.rs`: a **blue→red latency ramp** (distinct set of tokens, NOT `graph_edge_error`, which stays reserved for topology errors). Ships a named palette-independent ramp per theme (classic/terminal/mono must stay pairwise-legible).
- `src/ui.rs` `render_graph`: color each cable by `L/(N×AVG)` mapped onto the ramp; add a legend/status line "latency avg X / max Y (1 loop ≈ 190µs)" and a hover affordance on a sink "reads _X 1 loop behind".
- `src/app.rs`/`src/handler.rs`: a toggle to show/hide the latency coloring (default on) and status surfacing.
- `src/regression.rs` + `fixtures/`: snapshot matrix (latency fixtures spanning forward/back edges × classic/terminal/mono) asserting stable cable colors.

## Impact

- Affected specs: new capability `patch-latency-visualization`
- Affected code: `src/latency.rs` (new), `src/graph.rs`, `src/theme.rs`, `src/ui.rs`, `src/app.rs`, `src/handler.rs`, `src/regression.rs`, `fixtures/*`
- Non-goals: no patch reordering/optimization (follow-on change), no write-back, no per-circuit real µs truth (uses ramsize-proportional estimate, labeled), no `.ini` mutation.

## Non-goals

- No module-order optimization / reordering (separate follow-on change)
- No patch write-back / save-as
- No claim of exact per-circuit µs latency — the AVG is a ramsize-proportional *estimate* for relative coloring (units honored as loops/relative)
- No controller-instance remapping
