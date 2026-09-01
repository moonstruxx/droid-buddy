# Design: Graph layout rework

## Context

The signal-flow graph surface (`signal-flow-graph` capability) currently lays out circuits as a striped-topology cloud seeded by banner-cluster bands that stack vertically, with a weak cable spring that repulsion dominates. See `proposal.md` for motivation. The layout solver (`src/layout.rs`) is a one-shot deterministic force-directed solver seeded from topological depth + cluster bands + node-id hash (no RNG), converged then frozen, re-solved only on patch load (full) or node drag (damped local re-settle). The kitty path was introduced as an opt-in feature (`kitty-gfx`), with `GraphCamera` for world↔pixel mapping and box-drawing as the `supported()` fallback. The renderer owns layout and publishes `graph_node_rects` per frame for hit-testing.

## Goals / Non-Goals

**Goals:**
- Single dominant left→right axis — the graph reads as a horizontal pipeline and fills the canvas width.
- Cable springs dominate repulsion so edges read as the primary structure.
- The first circuit (`.ini` order) is pinned as the tip and never drifts.
- Banner groups render as enclosing rectangles with internal cohesion (content containers, not layout stripes).
- The screen-fit preserves aspect and prefers filling width, on both render paths.
- Kitty-gfx is the default renderer (in default features); box-drawing is the runtime fallback.
- Manual placements survive: `p` pins/unpins a node, drag pin-places a node.

**Non-Goals:**
- No change to parsing, validation, diff, or the patch model.
- No full layered-DAG rewrite beyond the single-axis bias.
- No animation or incremental re-layout beyond load/drag.
- No undo/history for manual pins; no cross-session persistence of pin positions (YAGNI).

## Decisions

### D1: Single-axis seed replaces the cluster-stripe seed

The initial positions are seeded by topological layer on one dominant axis rather than by cluster bands: `x = layer_index * HORIZONTAL_SPACING`, `y = within_layer_order * VERTICAL_SPACING` (plus the existing node-id hash for deterministic jitter to break ties). This replaces the previous "cluster bands stacked vertically" seed so the convergence target is a horizontal chain.

- Why: the canvas is wide; a horizontal chain uses the width and matches the "tip at left" reading order.
- Alternative considered (rejected): keep vertical cluster bands — produces a tall narrow graph that underuses width.
- Alternative considered (rejected): a strict layered-DAG layout — correct but a bigger rewrite and loses the spring character; the single-axis bias keeps the force-directed feel while biasing toward width.

### D2: Cable springs dominate repulsion

Raise the spring force influence (e.g. increase `SPRING_K`, keep `SPRING_REST`; optionally soften `REPULSION_STRENGTH`/`REPULSION_RADIUS`) so attraction between connected circuits dominates the repulsion-driven cloud. Tuning is done by adjusting existing constants; determinism (no RNG) is preserved.

- Why: the user asked for cables to be "spring-like power." Edges should be the primary reading of the layout.
- Alternative considered (rejected): switching to a pure spring-layout algorithm (no repulsion) — loses the non-overlap guarantee and the bounded-convergence contract.
- Risk: over-attraction collapses disconnected components onto one another. Mitigation: keep repulsion nonzero and validate with the existing single-axis regression fixtures.

### D3: Pin model — fixed anchors in the solver

Add a `pinned: &[usize]` (node indices, parallel to `graph.nodes`) parameter to `solve()`/`local_resettle()`. Pinned nodes are fixed anchors: their velocity is locked and their position never changes (they contribute to neighbor attraction/repulsion but never move). The tip — `graph.nodes[0]` in `.ini` order — is pinned by default.

- Why: "the tip shall be the first unit" and "manual placement must survive." A fixed anchor is the minimal solver-side mechanism.
- Alternative considered (rejected): a "preference weight" soft anchor — lets the tip drift, so it doesn't satisfy the pin requirement.
- App carries `pinned_circuits: HashSet<(String, usize)>` (NodeId-shaped) OR a plain `Vec<usize>` of node indices; the handler resolves hovered node → index and toggles membership. Node indices are stable across a build (parallel to `graph.nodes`) so an index set is sufficient and cheap; a NodeId set survives rebuilds but needs a re-map each build. Decision: use an index set in the solver and expose a NodeId-keyed set in `App` that maps to indices at solve time — keeps both rebuild-safety (NodeId) and solver simplicity (indices).

### D4: Cluster cohesion + enclosing rectangles

Banner groups keep being built by `collect_banner_groups` and turned into `Cluster` values, but two changes: (a) add an internal cohesion force — each cluster's members attract toward the cluster centroid (weak, below per-cable spring strength) so members cohere; (b) the renderer draws the cluster as an **enclosing rectangle** (union of member node rects + padding) instead of a stiff vertical band. Cohesion is a solver concern; the enclosing rectangle is a render concern (reuses the existing `graph_cluster_rects` union path).

- Why: the feedback called out "cluster = enclosing rectangle + cohesion," and it must not be a vertical layout stripe.
- Alternative considered (rejected): removing banner clusters from the layout — loses the semantic grouping worth keeping.
- The cohesion force is tuned to be weak enough that it never overrides the single-axis bias (clusters interleave by depth, not stripe).

### D5: Width-first aspect-preserving fit

The `GraphCamera` world→screen fit and the box-drawing fit both preserve aspect ratio and prefer filling the canvas width. When the graph's aspect is shorter than the canvas, the graph is scaled to fill width (and may overflow vertically with pan); when it is taller than the canvas, it scales to fit height. This replaces any current "fit the whole graph into the area" that center-crops small graphs.

- Why: "arrange horizontally to use most of the available canvas."
- Alternative considered (rejected): always fit height or always fit the entire bounding box — underuses width.

### D6: Kitty-gfx is default; box-drawing is the fallback

Move `kitty-gfx` into `Cargo.toml` **default features**. At runtime, `kitty_protocol::supported()` still gates the actual terminal capability check; when supported, the graph renders via kitty by default, and box-drawing is the fallback when unsupported (or the feature is compiled out via `--no-default-features`).

- Why: the feedback asks for "kitty default, box fallback." No new compile-time flag semantics; just the default set.
- Alternative considered (rejected): keep it opt-in — contradicts the feedback.

### D7: Manual pin key (`p`) and drag-to-place

`p` toggles pin/unpin on the hovered graph node (`hovered_graph_node`), mirroring the existing `x` toggle pattern. Dragging any node (pinned or unpinned) auto-pins it at the dropped position so manual placements survive; dragging a pinned node relocates its anchor. Unpin (`p` on a pinned node) releases it to the solver. The tip is pinned by default. The status line reflects the pin state change.

- Why: this is the direct delivery of "I can move circuits" — the previous drag-then-snap-back made manual arrangement impossible.
- Alternative considered (rejected): keep drag = temporary re-settle only (today's behavior) — manual placements don't survive, so it doesn't fix the complaint.

## Risks / Trade-offs

- **[Strong springs over-constrain]** → Keep repulsion nonzero; validate the single-axis and spring-dominance fixtures still converge within the energy/iteration budget. Adjust constants, not the bounded-convergence contract.
- **[Pin anchors create local stresses]** → Pinned nodes are fixed but still exert force on neighbors; the local re-settle budget already bounds the response. Ensure a pinned node's neighbors settle within `LOCAL_ITERATIONS`.
- **[Cohesion vs single-axis conflict]** → Tune cohesion below cable-spring strength so clusters interleave by depth; assert via a fixture that a multi-cluster patch does not revert to vertical stripes.
- **[Kitty default changes default build]** → The rasterizer deps are already unconditional, so no new dependency surface; `--no-default-features` still yields box-drawing. The insta snapshot path stays byte-identical (TestBackend never emits kitty).
- **[Determinism]** → All new forces derive from existing constants and the (unchanged) no-RNG seed; pins are deterministic inputs. Same patch + same machine → same layout.
