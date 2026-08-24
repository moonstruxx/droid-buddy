# Design: signal-flow-graph

## Context

droid_tui is a layered monolith: `patch.rs` parses `.ini` into a pure `Patch` model (components, sections, spans, occurrence/modifier indexes), `ui.rs` owns layout and publishes geometry (`component_rects`) for `handler.rs` hit-testing, and the event loop is draw→read→dispatch with no async runtime and no timer thread. The `g` prefix pattern (lazy 1 s timeout, checked on the next event) already routes `g v` to the embedded source viewer and `g r` to resize mode.

The main window is the controller representation — the user's map from on-screen components to physical hardware keys. It deliberately does not show wiring topology. DROID signal flow travels over virtual cables (`_NAME` registers): `output = _X` produces, any param referencing `_X` consumes. The user's working patches reach ~600 circuits, so whatever this view does, it must settle fast and stay still.

## Goals / Non-Goals

**Goals:**

- An optional third view focused purely on signal topology: circuits as nodes, virtual cables as edges, ComfyUI-style rendering.
- Cable extraction at parse time: comment-aware (definitions inside `#` lines ignored), expression-aware (`_NAME` tokens embedded in arithmetic are captured).
- Banner comment ranges (`# ---- Name ----` until the next banner or EOF) become cluster containers.
- Force-directed layout that converges to a stable arrangement and freezes — re-solves only on patch load or user node move.
- Topology validation: 1 source → n sinks valid; n sources → 1 sink flagged.
- Deterministic layout (same patch → same arrangement) at 600+ circuits.

**Non-Goals:**

- Continuous physics simulation / animation (no tick, no drift).
- Replacing the controller-panel view (it stays the primary surface).
- Editing the patch from the graph; hardware bridge; persistence of node positions (YAGNI).

## Decisions

### D1: Physics is a one-shot convergence solver, not a simulation

Run bounded iterations of spring attraction (along edges) + repulsion (between nodes) + friction damping until total kinetic energy drops below a threshold, then freeze positions. No continuous tick: the event loop stays synchronous draw→read→dispatch. Re-solve triggers are exactly two: patch load (full solve) and user node move (damped local re-settle from the moved position).

- *Why*: the user's stated requirement — physics finds a good arrangement, then everything must become stable fast; "no destruction by senseless drifting objects". A continuous simulation would burn CPU redrawing a frozen scene or introduce visible drift.
- *Alternative considered*: live simulation with damping — rejected: violates the stability requirement and adds timing complexity to a loop with no timer thread.

### D2: Cable extraction extends the boundary-aware scanner at parse time

The existing scanner already walks section values token-by-token with boundary awareness. Extend it to capture `_NAME` virtual-cable tokens from real section params only:

- `output = _NAME` registers a cable source for the section's circuit.
- Any other param value referencing `_NAME` (bare or embedded in expressions like `input = _X * -1 + _Y`) registers a sink reference.
- Comment lines (`# …`) are ignored entirely — real patches carry commented-out preamble cable maps that must not produce edges.
- Result stored as a cable index on `Patch` (cable name → source circuit + ordered sink circuits/params), alongside the existing indexes. Each index names its consumer (graph build).

- *Why*: single pass over values keeps the parser the sole owner of token grammar (DRY); parse-time index keeps the graph builder pure.
- *Alternative considered*: post-hoc regex over retained raw lines — rejected: second implementation of the token grammar risks divergence; comment handling would need re-implementation.

### D3: Banner ranges own circuits until the next banner or EOF

A comment banner (`# ---- Name ----`) starts a group; every circuit section from that line until the next banner (or end of file) belongs to it. Circuits before the first banner form an implicit unnamed group. Stored as ordered `(banner, section range)` pairs on `Patch`; the graph renders each named group as a cluster container titled with the banner text.

- *Why*: matches how the user's patches are structured semantically; clusters are the only viable density control at 600 circuits.

### D4: Topology validation is a graph-build step

During graph construction, each cable is checked: exactly one source with any number of sinks is valid; zero sources (dangling references) are flagged as warnings; two or more sources driving one cable name are flagged as invalid `n → 1` topology errors. Validation results travel with the graph model and are rendered as error highlights; they never block viewing.

- *Why*: the DROID convention is 1 source → n sinks; surfacing violations is the debugging value of the view.

### D5: New modules `src/graph.rs` and `src/layout.rs`, preserving the layered monolith

`graph.rs`: `GraphNode`/`GraphEdge` model, graph build from the cable index, topology validation. `layout.rs`: the convergence solver operating on the graph model, producing frozen positions. Both are pure modules with no terminal dependency — testable without rendering. `App` gains graph-view state (`showing_graph`, graph, positions, clusters); `handler.rs` wires keys/drag; `ui.rs` renders.

- *Why*: keeps the parse → model → layout → render layering; the solver and graph are the two pieces with real algorithmic substance and need focused test suites.

### D6: Observer-pattern event bus in `src/events.rs`

A thin synchronous event bus connects model, graph, and renderer: events for node moved, cable added/graph rebuilt, topology error. Single-threaded dispatch, no queueing, no async — observers register and are notified inline.

- *Why*: the graph view couples three concerns (state mutation, layout re-solve, re-render) that the rest of the app wires by direct calls; the bus keeps the re-solve triggers (load, move) decoupled from both the solver and the renderer, and gives topology errors a path to the status surface without handler↔renderer coupling.
- *Trade-off accepted*: in a single-threaded monolith a plain function call often suffices; the bus is deliberately minimal (enum events, synchronous notify) so it stays an abstraction with a job, not infrastructure.

### D7: View switching via the existing `g` prefix — `g g`

`g` then `g` opens the graph view (mirrors `g v` for the source viewer); `Esc` closes it and restores the previous view state. The prefix mechanism, lazy timeout, and cancel behavior are reused unchanged.

- *Why*: fits the keybinding spec's extensibility requirement (prefix pattern for extended commands) with zero new conflicts; mnemonic (`g` → graph).

### D8: ComfyUI-style rendering with box-drawing primitives

Nodes render as rounded frames with a title bar (circuit name), input ports on the left edge, output ports on the right edge. Edges are polyline curves approximated with box-drawing characters between port positions, color-coded by inferred cable type (control → cyan, audio → green, midi → magenta; unknown → neutral accent). Cluster containers are titled bordered areas enclosing member nodes. The graph view is a full-screen surface (like the source viewer), not an overlay mixed with panels. Colors come from theme tokens — no raw `Color::` literals.

- *Why*: ratatui has no vector curves; box-drawing polylines are the established terminal idiom and keep rendering deterministic for snapshot tests.
- *Note*: DROID cables are inherently untyped; the type is inferred from the producing circuit's category (clock/gate → control, note/midi → midi, otherwise audio/CV) and is a visual aid only — validation and topology never depend on it.

### D9: Scale strategy — spatial partitioning + deterministic seed

Repulsion uses uniform-grid cell hashing (rebuilt per iteration) so each node only repels against nodes in neighboring cells, cutting the 600-circuit case from ~180k all-pairs evaluations per iteration to near-linear. Initial positions are seeded deterministically from topological depth (sources left, sinks right, banner clusters vertically banded) plus a hash of the node id — no RNG, so the same patch always converges to the same arrangement on the same machine. Iteration count is capped; if the cap is hit before the energy threshold, the solver freezes anyway (stability beats perfection).

- *Why*: O(n²) all-pairs at 600 nodes is the hard scale constraint; determinism is testable and matches "stable fast again".
- *Alternative considered*: Barnes–Hut quadtree — rejected for now: cell hashing is simpler to implement and sufficient at this scale; can be swapped in behind the same solver interface if needed.

## Risks / Trade-offs

- **Edge density at 600 circuits**: even with clusters, a fully expanded graph can exceed readable terminal resolution. Mitigation: clusters as visual units first; interactive cluster collapse is a documented follow-up, not in this change.
- **Float determinism across platforms**: positions are only guaranteed identical on the same machine/toolchain; tests assert convergence, freeze stability, and same-machine determinism rather than hardcoded coordinates.
- **Drag re-solve cost**: a local re-settle after node move must be visibly fast; the damped re-settle uses fewer iterations and a tighter radius than the full solve. If it is still slow at 600 nodes, fall back to moving the single node without re-solving neighbors (recorded as an implementation-time decision).
- **Observer bus over-engineering risk**: kept minimal per D6; if it never grows a second subscriber per event, collapsing it back into direct calls is cheap.

## Open Questions

- None blocking. Implementation-time decisions (drag re-settle fallback, exact port slot allocation for nodes with many cables) are noted in the tasks.
