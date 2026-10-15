# Design

## Context

Three arrangements live in `src/layout.rs`:

- **Column** (default): deterministic width-aware dense-normalized depth columns, fixed outer controller/jack columns, no force relaxation — overlap-free by construction, but it honors `pins` **exactly** at whatever position they carry.
- **Force** (retained, `h` / `a` cycle): one-shot convergence from a layered Sugiyama seed. Forces: bundle-normalized cable springs (`tension` 0.05–0.5), size-aware repulsion — the legacy inverse-square falloff, now with a linear penetration ramp inside `required = 0.5·(extent_i + extent_j)` (D1/D2) — and weak per-cluster cohesion (`COHESION_K`, measured down from 0.01 to 0.0068, D3). The result passes through the **converging** non-overlap post-condition `separate_overlaps_converging` (D3), which replaced the wave-1 fixed 8-pass budget in task 1.4.
- **Sections** (sections-black-hole-solver): width-aware patch-order lanes, joint slice/section-hole/cable solve, **size-aware** repulsion with a linear penetration ramp on a graph-adaptive grid, and `separate_overlaps` as a hard post-condition.

Node bodies are `node_world_sizes()` = (`estimated_widths[i]`, `NODE_WORLD_H = 80`); the renderer draws exactly those extents and hit-tests the same world-space extents (ADR 40), so the body used by any overlap predicate *is* the frame the user sees. Widths reach ~496 (long title + two port markers), and sections mode clamps only its own drawn width, not force mode's.

The gap: force mode's repulsion is a **point** force with a cutoff (120) far below the required centre separation of two wide bodies (`(w_i + w_j)/2`, up to ~480). Such a pair is never pushed apart by the solver, and nothing else enforces separation, so the solve freezes with intersecting frames. `local_resettle` (drag) shares the same path.

**Measured (2026-10-15, `tests/force_overlap_probe.rs`, default tension and cost model)** — force-mode overlapping body pairs per fixture: `own_scale` 224, `graph_dependency_walk` 33, `cable_banner_combos` 7, `graph_latency_chain` 6, `own_two_ctrl` 4. The column arrangement reports 0 pairs on every fixture, which is the contract this change extends to force mode. Those numbers are the *pre-change* baseline (HEAD, before tasks 1.1–1.4); the post-change measurements, including the zero-pairs table the defaults are chosen from, are in D3.

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

**Measured reach (2026-10-15, task 1.5).** The ramp is real but it is not the whole story, and the difference is what task 1.4/1.5 had to fix. `fixtures/own_scale.ini`'s bodies are only ≤124 wide, so `required = 0.5·(w_i + w_j)` leaves the ramp nearly inert there — its residual overlap was never repulsion-driven. It is cohesion-driven: `COHESION_K` pulled ~85 banner-group members onto one centroid, and a batch of that density is a *separation* problem, not a force problem. Two consequences, both now measured rather than asserted: the force path's post-condition has to converge (D3), and the default cohesion has to sit at the top of the band where the pile still clears in a practical budget (D3's table). The slider (default `COHESION_K`) is the user's live lever inside that band.

### D2 — The repulsion grid cell adapts; the falloff keeps its own cutoff

The neighbour scan is a 3×3 cell walk, which is only correct when no repelling pair can be more than one cell apart. Since a repelling pair now exists up to `d < required ≤ max_extent`, the cell becomes `max(REPULSION_RADIUS, max_extent)` (the sections-mode rule). The falloff keeps its own `REPULSION_RADIUS` cutoff inside the force function, so widening the grid does **not** extend the long-range force's reach — it only lets the ramp see wide pairs. This keeps the positional change scoped to overlapping neighbourhoods.

### D3 — Non-overlap is a post-condition, not a tuning hope

The integrator is clamped (`clamp(accel, MAX_DISPLACEMENT)`, `FRICTION`, bounded iterations), so a strong spring can still drag one body onto another and lose the push. Sections mode already resolves this by enforcing the invariant explicitly (`separate_overlaps`, 8 bounded passes, cheapest-axis escape, deterministic `(i, j)` sweep). Force mode gets the same post-condition rather than a higher repulsion gain, because the invariant must hold *by construction*, not by tuning.

The force path cannot share the fixed budget, though (task 1.4): what a cohesion pile needs is a property of the knob tuple, not of the arrangement, and wave 1 measured the 8-pass budget leaving 240 pairs on the anchor at the then-default cohesion. So force mode runs `separate_overlaps_converging`:

1. read the residual over the **whole** pair set;
2. if it is zero, stop — the exit is a fresh reading, *never* the sweep's own visited-pair count (a sweep's push can create a pair it has already passed, so the visited count overstates the residual; that overstatement is the entire explanation of wave 1's "more passes make 0.01 worse" reading, and of its "64 passes at cohesion 0" need — the count-driven loop clears that anchor in 15);
3. otherwise sweep once, cycling three deterministic orders — node index, then bottom-up in y, then left-to-right in x — and go back to 1;
4. stop at `FORCE_SEPARATION_MAX_PASSES = 128` sweeps, the termination bound for a pile the greedy sweep cannot spread in any practical budget.

Sections keeps `SEPARATION_PASSES = 8` **and** the single node-index order (`separate_overlaps` is unchanged in behaviour, only its inner sweep moved into a shared helper), which is why its output stays byte-identical under the knob plumbing.

**Measured (2026-10-15, five-fixture set, default tension and repulsion gain).** Residual overlapping body pairs after `solve` on the 511-body `fixtures/own_scale.ini` anchor — the binding fixture of the set; the other four (`cable_banner_combos`, `graph_latency_chain`, `own_two_ctrl`, `graph_dependency_walk`) reach zero pairs at every cohesion and every cap in this table:

| cohesion \ cap | 8 | 12 | 16 | 17 | 18 | 19 | 20 | 24 | 28 | 32 | 64 | 128 | 256 | 384 | 512 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0.0 | 9 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | – | – |
| 0.005 | 6 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | – | – |
| 0.006 | 3 | 3 | 2 | 1 | 0 | 0 | – | – | – | – | – | 0 | – | – | – |
| 0.0065 | 3 | 3 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | – | – |
| **0.0068** | – | – | – | 2 | 0 | 0 | – | – | – | 0 | 0 | 0 | 0 | 0 | 0 |
| 0.0069 | 223 | – | – | 213 | 209 | 207 | – | – | – | – | – | 137 | 86 | **0** | 0 |
| 0.0070 | 242 | 240 | 238 | 233 | 234 | – | 234 | 230 | 228 | 225 | 212 | 200 | 157 | 131 | 91 |
| 0.0080 | 258 | 258 | 262 | 261 | 259 | – | 261 | 261 | 262 | 265 | 268 | 269 | 267 | 260 | 256 |
| 0.0100 | 240 | 245 | 244 | 243 | 242 | – | 243 | 243 | 246 | 247 | 251 | 257 | 264 | – | 274 |

Cells are residual pairs (the axis-aligned predicate `separate_overlaps` itself uses); `–` = not measured. Readings:

- **The default cohesion is `0.0068` (was `0.01`), and the frontier is a cost wall, not a breakage cliff.** Above `0.0068` the residual does fall with the cap, but the budget it needs explodes: `0.0069` needs ~300 sweeps where `0.0068` needs 18, and `0.0070`+ still has 91–274 pairs after 512 sweeps. So "the largest cohesion that reaches zero pairs" only has a meaning *together with the cap*, exactly as the task framed it — the pair is `(0.0068, 128)`, i.e. the largest cohesion that converges in a budget a live re-solve can afford.
- **The invariant is met, measured.** At that pair all five fixtures return zero overlapping body pairs. The cap is not doing the work: the worst fixture need is **18 sweeps** (the anchor at `0.006` and at `0.0068`; at `0.0065` and below it is ≤17), and 128 is 7× that. A clear layout pays one O(n²) read and no sweep at all.
- **The bracketing value is the largest that satisfies both bands.** `0.0068` is the top of the zero-pairs band; the same measurement run also shows the *floor*: on a synthetic graph of two 3-member clusters with no edges, cohesion bifurcates between `0.006` (max intra-cluster spread / min inter-cluster gap = 1.98 — members interleave and the clusters do not read as groups) and `0.0065` (0.97). A default at `0.006` is inside the zero-pairs band but fails the property test the repository already has for the mechanism, which is why the default sits at the top of the band rather than in the middle of it. Its headroom to the ceiling is 0.0001, so the pass cap — 7× the measured need — is what absorbs a layout whose own ceiling sits lower.
- **The sweep order is a residual-shaping knob, not a convergence switch.** Measured at the same caps with index-only, y-only and x-only sweeps: intermediate residuals differ by a pair or two (cap 1: 23 / 22 / 22 pairs on the anchor) but every ordering reaches zero on every fixture at ≤18 sweeps. The three-cycle is kept because the task calls for it and because it costs nothing, not because it is load-bearing.
- **Cohesion still coheres, and measurably more than at the old default's intent.** Mean distance from the banner-group centroid on the anchor: 3533 units at cohesion 0, **2870 at the 0.0068 default**, 2794 at the pathological 0.01 (whose layout is a pile of overlapping bodies). The knob does its job across its whole live range.
- **Cost.** O(n²) per sweep + O(n²) per residual read, so ~0.26 M pair checks per sweep at the 511-body anchor: a converging default solve (relaxation + 18 sweeps) measures ~145 ms debug / **13 ms release**, and the worst case — a user-dragged cohesion in the non-convergent regime, 128 sweeps — measures ~0.5 s debug / **38 ms release**. Only the non-convergent regime pays the cap. Task 3.5 gates the release figure.

**Cross-lane consequence (task 2.2):** the cohesion slider should top out at the measured ceiling, i.e. `COHESION_MAX ≈ 0.0068` — the default. Above it the panel offers the user a setting the solver cannot clear in any budget a live re-solve can afford, and the layout comes back visibly overlapping (`0.0069` → 137 pairs at the 128-sweep cap, `0.01` → 257).

### D4 — Drag: separation restricted to the active neighbourhood

`local_resettle` marks only nodes within `radius` as active; distant nodes act as anchors. The separation pass therefore runs with `movable[i] = active[i] && !pinned[i]`, so a drag can never shift the whole layout, and an overlapping active/inactive pair is corrected by the active node alone. Pinned nodes stay exactly where they are.

**Amended by measurement (task 3.1).** "Corrected by the active node alone" is not always possible: when a dragged body's clearance is blocked on every axis by bodies the pass may not move, no pass budget clears it — measured on `fixtures/own_scale.ini`, dropping the nearest node onto its partner leaves 6 active/inactive pairs that survive **8192** sweeps unchanged (an active 46×80 body wedged between two anchors 119 units apart in y, needing 160). That is a geometric cage, so the guarantee is scoped to what the pass can enforce: **every pair whose endpoints are both movable is clear** (asserted by `local_resettle_clears_overlap_in_the_active_neighbourhood`), while a body caged between pinned or out-of-neighbourhood bodies may keep its intersection. The alternative — letting a drag's separation displace anchors — was rejected because it breaks the drag's locality and the pin contract (ADR 45) to fix a state the user created by dragging into a gap narrower than the body; moving the cage's pinned bodies or unpinning them clears it. The spec delta carries the same scoping.

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
- **Separation cost** grows quadratically in each sweep, and the force path re-reads the residual between sweeps; bounded at `FORCE_SEPARATION_MAX_PASSES = 128` sweeps, measured 13 ms for a converging default solve on the 511-body anchor and 38 ms for a cap-exhausted one (release), and gated by a perf phase (task 3.5).
- **Pinned-pinned conflicts move pins** in column mode (D5). Only when they actually intersect, and it is the documented trade-off.
- **Sections byte-identity is the fragile part of this change** — the knob plumbing must not alter the sections forces. Guarded by the existing sections golden vector and its regression story.

## Migration / Rollout

No migration: knobs are session-only like the sections sliders, nothing is persisted, and no `.ini` is mutated. The arrangement cycle, config keys (`[layout] mode` / `ordering`) and keybindings are unchanged apart from `S` gaining force-mode meaning.

## Open Questions

- Should the force knobs persist in `[layout]` (like `mode`/`ordering`) or stay session-only? Currently session-only, mirroring the sections sliders.
- Should the column arrangement show the panel with disabled rows instead of only reporting that it has no forces? Currently: status hint only, matching the "no physics in column mode" framing.
