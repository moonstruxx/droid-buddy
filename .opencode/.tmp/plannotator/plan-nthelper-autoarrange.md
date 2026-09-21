# Plan: adopt nthelper's auto-arrange algorithm for the signal-flow graph

## Goal

Add a deterministic layered column layout to `src/layout.rs`, modeled on the
auto-arrange that nthelper uses for its routing editor, and make it the default
arrangement for the graph window. The existing force-directed layout stays
available as a non-default option, so nothing that depends on it is lost.

## Background: what nthelper does

`No-Such-Device/nt_helper` is a Flutter app for the Expert Sleepers Disting NT.
Its auto-arrange (`NodeLayoutAlgorithm`, `lib/core/routing/node_layout_algorithm.dart`)
is a custom layered layout with no force simulation and no randomness. Six passes:

1. Split edges into forward and feedback (backtracking) sets.
2. Longest-path depth over forward edges only, via memoized DFS.
3. Up to 10 refinement iterations: pull each feedback-edge source left to its
   destination's column, then re-enforce `v >= u + 1`.
4. Dense-normalize column ids to `0..N-1`.
5. Grid placement: per-column max width, column block centered horizontally,
   nodes stacked vertically in strict slot order (no overlap), snapped to a
   50px grid.
6. Physical inputs/outputs fixed at columns left and right of the algorithm
   block.

An O(C^2) overlap detector runs but only reports a severity score; it does not
move nodes. A separate cascade layout arranges a focused subset in a diagonal
stair-step order. Positions persist per preset.

## Current state of our solver

`src/layout.rs::solve` does a layered seed followed by a force relaxation:

- `seed_positions`: longest-path depth via bounded Bellman-Ford (caps cycle
  growth; depth can exceed `n` on cycles), barycenter crossing-minimization
  sweeps (`CROSSING_SWEEPS` 8) for within-layer order, snapped to `GRID_SNAP`.
- `run_iterations`: up to `SOLVE_ITERATIONS` 60 force iterations (springs,
  grid-hashed repulsion, friction, energy-threshold early exit), with `tension`
  spring stiffness and `pinned` anchors.
- `local_resettle`: damped radius-limited re-settle after a node drag.

The seed is already the same family as nthelper's algorithm. What nthelper adds:
width/height-aware placement (no column collisions) and slot-order stacking as
an overlap guarantee by construction.

## Decisions (annotated via Plannotator)

1. **Replace or coexist?** Coexist. The nthelper column layout becomes the
   default; the force layout stays available. This implies a toggle, either a
   config option or a keybinding, to switch between the two arrangements.
2. **Crossing minimization vs slot-order stacking.** Keep both, configurable,
   with strict-order stacking as the default. The config switch chooses between
   barycenter crossing minimization and nthelper's strict slot order.
3. **Fate of tension and pins.** Keep them. Tension and pins remain features of
   the force layout, which survives as the non-default arrangement, so nothing
   is lost. In the column layout they do not apply (no springs to tune, no
   re-settle to anchor), or pins act as fixed-position anchors if that is
   cheap.
4. **Back-edge reduction.** Keep the capped Bellman-Ford depth computation.
   The column layout uses the existing cycle-safe depth; nthelper's
   forward/feedback pull loop is not adopted.

## Proposed changes

### 1. Column layout path in the solver

Add a column-placement function alongside the existing force path, reusing the
layered seed:

- Reuse the existing longest-path depth (capped Bellman-Ford) as the column
  assignment; dense-normalize columns to `0..N-1`.
- No force relaxation after the seed: the layered seed plus grid placement is
  the final column layout.
- Keep `solve` as a pure deterministic function: same patch, same machine,
  same layout. The force path (`run_iterations`, `local_resettle`) stays intact
  for the non-default arrangement.

### 2. Grid placement

Replace the uniform-spacing placement with width-aware column stacking:

- Estimate node width from circuit name length and port label lengths.
- Per-column max width; center each column block horizontally.
- Stack nodes vertically in deterministic order with spacing, so nodes in a
  column never overlap.
- Snap all coordinates to the `GRID_SNAP` grid.

The layout solver currently knows nothing about node sizes; the renderer
computes them. We need to either pass estimated sizes into the solver or keep
the estimator inside it.

### 3. Within-layer ordering, configurable

Two orderings for the vertical stacking, switched by config:

- Strict slot order (nthelper style, the default): nodes stack in
  deterministic order, guaranteeing no vertical overlap.
- Barycenter crossing minimization (existing sweeps): fewer edge crossings,
  kept as the non-default option.

### 4. Controller and jack nodes

nthelper fixes physical I/O at columns left/right of the algorithm block. Our
graph already has controller and jack nodes (`NodeKind::Controller`,
`InputJack`, `OutputJack`). Open sub-decision: fixed outer columns or the same
layering as circuit nodes. Default proposal: fixed outer columns, mirroring
nthelper, with circuit columns between them.

## Test and verification impact

- Existing force-layout tests stay: determinism, finiteness, freeze,
  local-resettle, cluster seed, energy convergence. The force path is
  unchanged.
- New column-layout tests: column assignment, dense normalization, no-overlap
  property, grid snapping, determinism, strict-order vs barycenter ordering.
- Snapshot/golden tests whose expected positions come from the default layout
  switch to the column layout and need regeneration.
- Graph-drag interaction tests in `handler.rs` still exercise `local_resettle`
  on the force path; the column layout has no drag re-settle unless pins get
  fixed-position anchor meaning.
- Perf gate: the column path is cheaper (no iteration loop), so
  `GRAPH_SOLVE_BUDGET` stays comfortably within budget.

## Suggested task breakdown

1. Add the column-layout function to `src/layout.rs` (layered seed + dense
   columns + width-aware grid placement, no relaxation).
2. Add the configurable within-layer ordering (strict-order default, barycenter
   option).
3. Wire the default/toggle between column layout and force layout (config +
   keybinding), including controller/jack placement.
4. Keep tension, pins, and drag behavior on the force path; decide pin meaning
   on the column path.
5. Update tests, snapshots, and docs.