# Design

## Context

Three arrangements live in `src/layout.rs`:

- **Column** (default): deterministic width-aware dense-normalized depth columns, fixed outer controller/jack columns, no force relaxation — overlap-free by construction, but it honors `pins` **exactly** at whatever position they carry.
- **Force** (retained, `h` / `a` cycle): one-shot convergence from a layered Sugiyama seed. Forces: bundle-normalized cable springs (`tension` 0.05–0.5), point repulsion (`REPULSION_STRENGTH/d²`, cutoff `REPULSION_RADIUS = 120`, grid cell = that radius), weak per-cluster cohesion (`COHESION_K = 0.01`). **No** separation post-condition, **no** size awareness.
- **Sections** (sections-black-hole-solver): width-aware patch-order lanes, joint slice/section-hole/cable solve, **size-aware** repulsion with a linear penetration ramp on a graph-adaptive grid, and `separate_overlaps` as a hard post-condition.

Node bodies are `node_world_sizes()` = (`estimated_widths[i]`, `NODE_WORLD_H = 80`); the renderer draws exactly those extents and hit-tests the same world-space extents (ADR 40), so the body used by any overlap predicate *is* the frame the user sees. Widths reach ~496 (long title + two port markers), and sections mode clamps only its own drawn width, not force mode's.

The gap: force mode's repulsion is a **point** force with a cutoff (120) far below the required centre separation of two wide bodies (`(w_i + w_j)/2`, up to ~480). Such a pair is never pushed apart by the solver, and nothing else enforces separation, so the solve freezes with intersecting frames. `local_resettle` (drag) shares the same path.

**Measured (2026-10-15, `tests/force_overlap_probe.rs`, default tension and cost model)** — force-mode overlapping body pairs per fixture: `own_scale` 224, `graph_dependency_walk` 33, `cable_banner_combos` 7, `graph_latency_chain` 6, `own_two_ctrl` 4. The column arrangement reports 0 pairs on every fixture, which is the contract this change extends to force mode. The probe's per-fixture ratchet is superseded by task 3.1's zero-pairs invariant.

The `S` panel (`src/gui/graph.rs`) is gated on `LayoutMode::Sections`, so force mode's repulsion and cohesion are unreachable at runtime — only `Alt+[`/`Alt+]` (tension) is live.

## Goals / Non-Goals

**Goals:**
- No two node bodies overlap after any path that produces positions: force `solve`, drag `local_resettle`, column `solve_columns_pinned` (pins included), sections `solve_sections` (already true).
- Force mode gains live knobs — repulsion and cohesion — alongside the existing tension, exposed in the `S` panel in force mode the same way the sections sliders work.
- Determinism stays keyed on an explicit knob tuple: same patch + same machine + same knobs → identical positions.
- Column and sections arrangements keep their current output; the sections sliders, ranges and Arrange semantics are unchanged.
- Panel geometry unchanged (same three rows plus Arrange), so no new chrome work.

**Non-Goals:**
- Re-tuning force mode into a new visual character: the change must only add separation where bodies actually intersect, leaving the arrangement's overall spread intact.
- A tether or anneal semantics for force mode (sections-only concepts).
- Persisting knob values across patches/sessions (mirrors the sections sliders, which are session-only).
- Changing node sizing (`estimated_widths`, `NODE_WORLD_H`), the sections lane cap, or the camera/hit-test geometry (ADR 40 stands).
- RNG, or any non-deterministic ordering.

## Decisions

### D1 — Add a penetration ramp *inside* `required`; keep the legacy falloff beyond it

Force-mode repulsion becomes:

```text
required = 0.5 * (extent_i + extent_j)          // extent = max(width, height)
if d < required:                                // bodies intersect
    mag = MAX_DISPLACEMENT * min(1.0, (required - d) / REPULSION_RADIUS)
else:                                           // unchanged legacy behaviour
    mag = REPULSION_STRENGTH / d²   (0 beyond REPULSION_RADIUS)
mag *= knobs.repulsion                          // gain, default 1.0
```

Rationale: the inverse-square falloff is what spaces *non-overlapping* neighbours today, so removing it would change force mode's whole character and could collapse long chains; the ramp engages only where bodies actually intersect and is bounded by `MAX_DISPLACEMENT`, so it cannot explode the layout. The formula is literally the sections-mode ramp, so both arrangements share one separation language.

Rejected alternative: replace force-mode repulsion with the pure sections ramp (no long-range term). It is simpler, but it removes the spread that keeps clusters readable in force mode and would make the golden re-bless a rewrite rather than a perturbation.

### D2 — The repulsion grid cell adapts; the falloff keeps its own cutoff

The neighbour scan is a 3×3 cell walk, which is only correct when no repelling pair can be more than one cell apart. Since a repelling pair now exists up to `d < required ≤ max_extent`, the cell becomes `max(REPULSION_RADIUS, max_extent)` (the sections-mode rule). The falloff keeps its own `REPULSION_RADIUS` cutoff inside the force function, so widening the grid does **not** extend the long-range force's reach — it only lets the ramp see wide pairs. This keeps the positional change scoped to overlapping neighbourhoods.

### D3 — Non-overlap is a post-condition, not a tuning hope

The integrator is clamped (`clamp(accel, MAX_DISPLACEMENT)`, `FRICTION`, bounded iterations), so a strong spring can still drag one body onto another and lose the push. Sections mode already resolves this by enforcing the invariant explicitly (`separate_overlaps`, 8 bounded passes, cheapest-axis escape, deterministic `(i, j)` sweep). Force mode gets the same post-condition rather than a higher repulsion gain, because the invariant must hold *by construction*, not by tuning.

Cost: O(n²) per pass, bounded at 8 passes — ~0.13 M pair checks per pass at the 511-node scale anchor. Acceptable, and gated (task 3.5).

### D4 — Drag: separation restricted to the active neighbourhood

`local_resettle` marks only nodes within `radius` as active; distant nodes act as anchors. The separation pass therefore runs with `movable[i] = active[i] && !pinned[i]`, so a drag can never shift the whole layout, and an overlapping active/inactive pair is corrected by the active node alone. Pinned nodes stay exactly where they are.

### D5 — Pins: exact honoring yields only to the invariant

The column path honors pins exactly, including off-grid positions carried from a force solve or a drag; two such pins (or a pin and a slot neighbour) can intersect, and two immovable bodies cannot be separated. Policy:

1. Run the separation pass with all pins immovable (the common case: pins are the only bodies that could conflict, and unpinned neighbours give way).
2. If pairs remain, run one final pass with every body movable — the no-overlap invariant wins, and only intersecting pinned bodies actually move.

Rejected alternative: drop pins when the arrangement changes. Simpler, but it silently discards the user's manual placement, which is exactly what pins exist to preserve (ADR 45 carries pins by `NodeId` across rebuilds for that reason). The trade-off is documented in the code and in the spec scenario.

### D6 — `SolveKnobs` instead of more positional arguments

The solver already carries `#[allow(clippy::too_many_arguments)]` ("one param per solver knob"); adding repulsion and cohesion would make `run_iterations_with_forces` eleven arguments and force two more arguments through `solve`, `solve_sections` and `local_resettle`. Instead:

```rust
#[derive(Clone, Copy, Debug)]
pub struct SolveKnobs {
    pub tension: f32,     // cable spring stiffness
    pub slice_k: f32,     // sections: slice spring
    pub section_k: f32,   // sections: section-hole spring
    pub repulsion: f32,   // repulsion gain (1.0 = default)
    pub cohesion: f32,    // force-mode cluster cohesion
}
impl Default for SolveKnobs { /* DEFAULT_TENSION, SLICE_K, SECTION_HOLE_K, 1.0, COHESION_K */ }
```

The struct names the knob tuple, which is now the determinism key ("same patch + same knobs → same layout"), and removes the argument-count pressure for future knobs. Cost: mechanical call-site churn in `src/app.rs` and the layout/regression tests, guided by the compiler; `SolveKnobs::default()` reproduces today's defaults, so the sections path stays behaviour-preserving.

### D7 — The panel keeps its geometry and switches its rows by mode

`SlidersPanel` already has exactly three rows plus Arrange, and both modes need exactly three knobs, so no geometry changes:

| arrangement | row 1 | row 2 | row 3 | Arrange |
|---|---|---|---|---|
| `Force` | Repulsion `0.0..=REPULSION_GAIN_MAX`, default 1.0 | Cohesion `0.0..=COHESION_MAX`, default `COHESION_K` | Cable `TENSION_MIN..=TENSION_MAX` (= `App.tension`, in sync with `Alt+[`/`Alt+]`) | re-solve from the seed, `Arranged (force)` |
| `Sections` | Slice (unchanged) | Section (unchanged) | Cable (unchanged) | anneal from current positions, `Arranged (sections)` (unchanged) |
| `Column` | no panel; status reports that the arrangement has no forces | | | `a` keeps its arrange-cycle meaning |

A slider drag re-solves live, exactly as in sections. `showing_sliders` stays a single toggle; the panel simply only lays out in the two force-bearing arrangements.

### D8 — Golden vectors are re-blessed; invariants replace byte-identity

Size-aware repulsion necessarily moves force-mode positions, so `force_mode_positions_are_unchanged_by_size_aware_repulsion` (a sections-change guard asserting force mode was untouched) inverts and is replaced. The new guards are stronger than byte-identity:

- **non-overlap invariant** — zero intersecting body pairs after solve / drag / pinned column solve (task 3.1);
- **determinism per knob tuple** — two solves with identical knobs return identical positions (task 3.2);
- **knob effect** — changing repulsion, and changing cohesion, each measurably changes the layout, so a knob cannot silently become a no-op; and
- **sections/column byte-identity** — the existing sections golden vector and the force-mode seed-bound guard stay untouched, so the knob plumbing must not perturb them.

## Risks

- **Force mode looks different on every patch.** Accepted and intended: the arrangement was rendering intersecting frames. Where a wide node now pushes its neighbours apart, its cluster spreads slightly more than before.
- **A dense bundle of wide nodes may spread more than expected** because the ramp is stronger inside `required` than the old falloff. Mitigation: the gain slider (default 1.0 keeps the ramp bounded at `MAX_DISPLACEMENT`); if the default reads too loose, the knob is the tuning surface, not a code change.
- **Separation cost** grows quadratically; bounded at 8 passes and gated by a perf phase.
- **Pinned-pinned conflicts move pins** in column mode (D5). Only when they actually intersect, and it is the documented trade-off.
- **Sections byte-identity is the fragile part of this change** — the knob plumbing must not alter the sections forces. Guarded by the existing sections golden vector and its regression story.

## Migration / Rollout

No migration: knobs are session-only like the sections sliders, nothing is persisted, and no `.ini` is mutated. The arrangement cycle, config keys (`[layout] mode` / `ordering`) and keybindings are unchanged apart from `S` gaining force-mode meaning.

## Open Questions

- Should the force knobs persist in `[layout]` (like `mode`/`ordering`) or stay session-only? Currently session-only, mirroring the sections sliders.
- Should the column arrangement show the panel with disabled rows instead of only reporting that it has no forces? Currently: status hint only, matching the "no physics in column mode" framing.
