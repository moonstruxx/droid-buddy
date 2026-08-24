# OpenSpec Tasks: signal-flow-graph

Wave plan (file-disjointness + depends_on, capped at maxConcurrent: 3):
`[1.1]` -> `[1.2, 2.1]` -> `[1.3, 2.2, 3.1]` -> `[2.3, 3.2, 4.1]` -> `[4.2, 5.1]` -> `[4.3, 5.2]` -> `[5.3]` -> `[6.1]`

Notes:
- Tasks 1.1/1.2/1.3 share `src/patch.rs` — strictly sequential even where `depends_on` alone would allow overlap.
- Tasks 2.1/2.2/2.3 share `src/graph.rs`; 3.1/3.2 share `src/layout.rs`; 5.1/5.2/5.3 share `src/ui.rs` — each group strictly sequential.
- No new dependencies; all work stays in the single crate. Unwrap/expect only inside test modules.
- Design references: D1 (convergence solver), D2 (cable extraction), D3 (banner ranges), D4 (topology), D6 (event bus), D9 (partitioning + seed).

## 1. Signal-flow parsing (patch.rs)

- [x] 1.1 Extend the boundary-aware scanner to extract virtual cables from real section params: `output = _NAME` registers a cable source for the section's circuit; `_NAME` references in any other param value (bare or embedded in expressions) register sink references; definitions in comment lines are ignored; store a cable index (cable name → source circuit + ordered sink references) as a `Patch` field. Verify: parser tests for `output =` sources, expression-embedded sinks, commented-definition exclusion, `_ENV…`-style internal names inside comments not leaking, and unchanged existing fixtures/results <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/patch.rs] -->
- [x] 1.2 Banner-range grouping: comment banners (`# ---- Name ----`) own the circuit range from themselves until the next banner or EOF; circuits before the first banner form an implicit unnamed group; store ordered banner→section-range pairs on `Patch`. Verify: tests for multiple banners, pre-first-banner circuits, banner-at-EOF, and repeated-section attribution <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/patch.rs] -->
- [x] 1.3 Unit tests for cable extraction + banner grouping against real fixtures (incl. a fixture exercising 1→n fan-out, commented-vs-real cable maps, expression-embedded tokens): extraction, grouping, and their combination. Verify: full `cargo test` green including new tests <!-- agent: horst-engineer.build, depends_on: [1.1, 1.2], touches: [src/patch.rs] -->

## 2. Graph model + topology (new src/graph.rs)

- [x] 2.1 Create `src/graph.rs` with the GraphNode/GraphEdge model: circuits (+ instances) → nodes, cable source→sinks → directed edges; pure `build_from_patch(&Patch)` constructor consuming the cable index and banner groups (clusters). Verify: build tests — node set matches circuits, edges match cable fan-out, clusters match banner ranges <!-- agent: api-engineer.build, depends_on: [1.1], touches: [src/graph.rs] -->
- [x] 2.2 Topology validation as a graph-build step: exactly one source per cable with n sinks is valid; dangling references (no source) flagged as warnings; multiple sources on one cable flagged as invalid n→1 topology errors; results travel with the graph model and never block viewing. Verify: tests for valid fan-out, dangling warning, n→1 error, and mixed cases <!-- agent: api-engineer.build, depends_on: [2.1], touches: [src/graph.rs] -->
- [x] 2.3 Graph model + topology test suite through the public `build_from_patch` entry point with fixtures: model shape, edge directions, cluster membership, all validation states. Verify: full `cargo test` green including new tests <!-- agent: horst-engineer.build, depends_on: [2.1, 2.2], touches: [src/graph.rs] -->

## 3. Force-directed layout solver (new src/layout.rs)

- [x] 3.1 Create `src/layout.rs` with the convergence solver: spring attraction along edges + repulsion between nodes + friction damping, bounded iterations, freeze when total kinetic energy < threshold (freeze at iteration cap regardless — stability beats perfection); uniform-grid cell hashing for repulsion (design D9); deterministic seed from topological depth (sources left, sinks right, clusters vertically banded) + node-id hash, no RNG; two entry points: full solve (patch load) and damped local re-settle (node move). Verify: solver produces finite positions for disconnected, cyclic, and 600-node synthetic graphs without panicking <!-- agent: api-engineer.build, depends_on: [2.1], touches: [src/layout.rs] -->
- [x] 3.2 Solver test suite: convergence within the iteration cap, freeze stability (positions unchanged across repeated queries after freeze), same-machine determinism (same input → identical positions), local re-settle terminates faster than a full solve and leaves distant nodes essentially unmoved. Verify: full `cargo test` green including new tests <!-- agent: horst-engineer.build, depends_on: [3.1], touches: [src/layout.rs] -->

## 4. App state + observer integration (app.rs, events.rs, handler.rs)

- [x] 4.1 Graph view state in `App`: `showing_graph`, graph model, frozen node positions, cluster rects; initialize from a fresh solve on open after `load_patch`; closing preserves panel/source-viewer state. Verify: unit tests for defaults, open/close lifecycle, and state reset on patch load <!-- agent: api-engineer.build, depends_on: [2.1, 3.1], touches: [src/app.rs] -->
- [x] 4.2 Observer event bus in new `src/events.rs` (design D6): enum events for node moved, graph rebuilt, topology error; synchronous single-threaded dispatch, no queueing; wire the re-solve triggers (load, move) and topology-error reporting through it. Verify: bus tests — subscriber notification, event ordering, no-op without subscribers <!-- agent: api-engineer.build, depends_on: [4.1], touches: [src/events.rs] -->
- [x] 4.3 Handler wiring: `g g` opens the graph (runs full solve via the bus, mirrors `g v` prefix handling), node drag/move updates position and triggers damped re-settle, `Esc` closes and restores panel focus with selection/source state intact; prefix cancel/timeout behavior unchanged. Verify: handler tests through `handle_event` for open/close, drag re-solve trigger, Esc restore, and prefix non-interference <!-- agent: api-engineer.build, depends_on: [4.1, 4.2], touches: [src/handler.rs] -->

## 5. Renderer (ui.rs)

- [x] 5.1 ComfyUI-style node frames in `src/ui.rs`: rounded frame + title bar (circuit name), input ports on the left edge, output ports on the right edge; cluster containers from banner groups (titled bordered areas enclosing members); graph view as a full-screen surface with empty-patch message; all colors via theme tokens. Verify: frame render tests — node/ports/cluster geometry at wide and narrow sizes, empty state <!-- agent: layout-designer-engineer.build, depends_on: [2.1, 4.1], touches: [src/ui.rs] -->
- [x] 5.2 Edge rendering: polyline curves with box-drawing characters between port positions, color-coded by inferred cable type (control cyan / audio green / midi magenta / unknown neutral accent, design D8); topology errors highlighted; edges clipped cleanly at view bounds. Verify: frame tests assert edge character placement for straight, crossing, and cluster-spanning cables <!-- agent: layout-designer-engineer.build, depends_on: [5.1], touches: [src/ui.rs] -->
- [x] 5.3 Graph render snapshot/visual tests: node + port + cluster faces, edge colors, topology-error highlight, narrow-terminal degradation — via the existing `TestBackend` + `insta` harness used for the panels/viewer. Verify: `cargo insta test --check` green including new snapshots <!-- agent: horst-engineer.build, depends_on: [5.1, 5.2], touches: [src/ui.rs] -->

## 6. Verification

- [ ] 6.1 Full gate: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test` (incl. `cargo insta test --check`), `cargo build --release --locked` — all four exit 0. Verify: gate output clean; no new warnings <!-- agent: horst-engineer.fast, depends_on: [1.3, 2.3, 3.2, 4.3, 5.3], touches: [] -->
