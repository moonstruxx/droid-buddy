# Proposal

## Why

The force arrangement renders **overlapping node bodies**. Two mechanisms produce it:

1. **Point repulsion cannot separate wide bodies.** Force mode's repulsion (`src/layout.rs::repulsion_force`) has magnitude `REPULSION_STRENGTH / d²` (1500/d²) and a hard cutoff at `REPULSION_RADIUS = 120`, but a node body is `layout::node_world_sizes()` = (`estimated_widths[i]`, `NODE_WORLD_H = 80`) and estimated widths reach ~496 on real patches (a long circuit title plus two port markers). The separation at which two bodies stop overlapping is `required = (w_i + w_j) / 2`, which for two wide nodes is ~480 — four times the cutoff. Such a pair is never pushed apart at all, so the solve freezes with the frames intersecting. The sections arrangement was fixed for exactly this (`repulsion_force_size_aware`, a linear penetration ramp on a graph-adaptive grid); force mode was deliberately left untouched.
2. **Non-overlap is not a post-condition of the force path.** Sections mode enforces it as an explicit post-condition (`separate_overlaps`, 8 bounded deterministic passes) *in addition to* the size-aware force, because the springs can still lose. Force mode has no such pass, and `local_resettle` — the node-drag re-settle — runs the same force path, so dragging a node can leave its neighbours intersecting too. The column arrangement honors pinned positions exactly, including off-grid positions carried over from a force solve or a drag, which can overlap the slots around them.

Separately, the `S` sliders panel is gated on `LayoutMode::Sections` (`src/gui/graph.rs:785`), so force mode has **no interactive control of its own forces**: repulsion strength and cohesion are compile-time constants (`REPULSION_STRENGTH`, `COHESION_K`) that can only be changed by editing the source, and the only live knob is `Alt+[`/`Alt+]` for tension. The user's expectation — sliders that tune the forces plus an Arrange action — is met in sections mode only.

## What Changes

- **Size-aware force-mode repulsion.** Inside a pair's required separation the solver applies the same bounded linear penetration ramp sections mode uses; beyond it the retained inverse-square falloff keeps the arrangement's existing spread (its own `REPULSION_RADIUS` cutoff is kept, so only overlapping pairs change behaviour). The repulsion grid cell adapts to the largest node extent so the 3×3 neighbour scan can never miss a repelling pair.
- **Non-overlap becomes a post-condition of the force path**: a bounded `separate_overlaps` pass runs after `solve` and after `local_resettle` (with inactive nodes immovable, so a drag never drags the whole layout), and over the column arrangement's placed positions. Pins stay immovable in the first passes; if two pinned bodies still intersect, a final all-movable pass guarantees the invariant.
- **Force mode gains runtime knobs**: repulsion gain and cohesion strength become `App` fields (session-only, like the sections sliders), threaded through a new `layout::SolveKnobs` bundle together with the existing tension, slice and section forces.
- **The `S` panel becomes arrangement-aware**: force shows Repulsion / Cohesion / Cable; sections keeps Slice / Section / Cable (behaviour unchanged); the column arrangement has no forces and reports that in the status line instead of showing a panel. Panel geometry is unchanged (three rows plus Arrange), so no new layout work.
- **Arrange is arrangement-aware**: sections keeps the anneal from current positions (`Arranged (sections)`); force re-solves from the seed and reports `Arranged (force)`; `Shift+A` follows the active arrangement.
- **Force-mode position golden vectors are re-blessed.** Size-aware repulsion necessarily moves force-mode positions; the determinism contract is preserved and re-asserted per knob tuple, and the non-overlap invariant becomes the new regression guard.

## Capabilities

### Modified Capabilities

- `signal-flow-graph`: the **Convergence-based layout** requirement gains non-overlap guarantees (no two node bodies overlap after a solve, after a drag re-settle, or when pinned) and the runtime repulsion/cohesion parameters.
- `graph-arrange`: a new **Arrangement-aware force sliders** requirement covers the panel in force mode, its knob set, and its Arrange semantics.

### Unchanged

- `graph-sections-layout`: the sections sliders, their ranges and defaults, and the sections layout itself keep their current behaviour; the panel geometry is shared rather than duplicated.

## Impact

- Touched: `src/layout.rs` (size-aware force repulsion, separation post-conditions, `SolveKnobs`), `src/app.rs` (repulsion/cohesion fields, knob plumbing, force arrange), `src/config.rs` (knob defaults), `src/handler.rs` (`S` panel routing and status, Arrange key), `src/gui/graph.rs` (mode-aware panel), `src/help.rs` (key tables), `ARCHITECTURE.md` / `DESIGN.md`.
- Behaviour: force-arrangement positions change on every patch (deliberate); the column default and the sections arrangement are unchanged; determinism holds per knob tuple (same patch + same machine + same knobs → identical positions).
- Cost: the separation pass is O(n²) per pass with 8 passes bounded (~0.13 M pair checks per pass at the 511-node scale anchor). A perf-gate phase guards the force solve plus separation.
- Tests that pin force-mode positions must be re-blessed; assertions that force mode is byte-identical to the pre-size-aware solver must be replaced by the new invariants.
