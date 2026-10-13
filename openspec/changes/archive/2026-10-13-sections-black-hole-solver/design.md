# Design

## Context

See `proposal.md` - Why. The current state: two layout solvers in `src/layout.rs`:
- **Column arrangement (default)**: deterministic, width-aware dense-normalized circuit-depth columns, fixed outer controller/jack columns, strict/barycenter within-column order. No physics, guaranteed no-overlap.
- **Force-directed (retained, `h` toggle)**: one-shot convergence ≤60 iterations from a layered Sugiyama seed (topological depth + barycenter crossing-minimization). Forces: cable springs (bundle-normalized, tension 0.05–0.5), grid-hashed repulsion (radius 120), weak centroid cohesion (`COHESION_K=0.01`). Cluster rectangles are render-only.

The column layout became default because force output was unreadable at patch scale (ADR 29). The retained force solver is the starting point for the sections mode.

## Goals / Non-Goals

**Goals:**
- Sections mode as a third `[layout] mode` alongside `column` and `force`
- Joint black-hole solve: slice springs + section-hole springs + cable springs + size-aware repulsion in one `run_iterations`
- Fixed section holes from patch order (infinite mass, never move)
- Live sliders panel + Arrange button for tuning
- Determinism per force triple (same patch + same triple → same layout)
- Column remains default; `h`/`a` cycle gains sections step

**Non-Goals:**
- No container rectangles or visible section boxes
- No two-phase container packing
- No RNG or non-determinism
- No up/down arrange direction
- No section-hole strength slider initially (fixed constant)
- No persistence of slider values across patch loads

## Decisions

### D1 — Joint solve in one `run_iterations` with four forces

Rather than two-phase packing or separate passes, extend `run_iterations` with a mode branch that adds two spring terms alongside the existing cable springs and repulsion:

```rust
// For each node i:
accel = cable_spring_force(i) + slice_spring(i) + section_hole_spring(i) + repulsion(i) + (cohesion_if_enabled)
```

**Slice spring**: each circuit `i` has a target `x = slice_index(i) * HORIZONTAL_SPACING` (patch order, at least one slice per circuit). Spring pulls toward `(target_x, current_y)` with stiffness `SLICE_K`.

**Section-hole spring**: for each banner group `g`, compute a fixed anchor `hole_g = mean(slice_index(members)) * HORIZONTAL_SPACING` at the first `solve` call. Member `i` in group `g` feels spring toward `(hole_g.x, hole_g.y)` with stiffness `SECTION_HOLE_K`. Anchors never move.

**Why**: One solver, same grid-hashed repulsion, same determinism contract. No new pass or data structure.

**Alternatives**: Two-phase (containers first) — more code, containers strand space, two convergence targets.

**As-built correction (seed pathology, measured).** The implementation initially reused `seed_positions()` — the force-mode Sugiyama seed — for sections mode too. That seed is `x = topological_depth(i) * HORIZONTAL_SPACING`, and `topological_depth` is a bounded Bellman-Ford run for `n` passes whose depth on a **cyclic** graph grows by roughly the cycle length per pass and can far exceed `n` (the function's own comment says so). Measured on `fixtures/own_scale.ini` (511 nodes, cyclic): max depth ≈ 245,266, so the seed alone is **19,621,280** wide; the clamped integrator (`MAX_DISPLACEMENT` 20/component, `FRICTION` 0.5 ⇒ |v| ≤ 40/iteration, `SOLVE_ITERATIONS` 60 ⇒ ≤2400 total travel) can only move it one step, giving a final bbox width of **19,621,300** — a horizontal smear ~480× beyond the furthest slice target. Column mode is unaffected (530 × 60,840) because it dense-normalizes depth into columns `0..N-1`.

This is why sections mode must seed **by patch order** (`x = slice_index(i) * HORIZONTAL_SPACING`, `y` = within-slice order × `VERTICAL_SPACING`) — which is what D1 already specifies — rather than by topological depth. The force path keeps the old seed unchanged (golden-vector protected); its own uncapped-depth instability is a **pre-existing** bug filed separately as bead `droid_tui-68g`. A scale guard test now pins sections mode to a bbox proportional to patch size on `own_scale.ini`.

**Lane geometry: width-aware, wrapped into rows (user decision).** Two further measured facts force a change beyond the plain patch-order seed: (a) the clamped integrator can travel at most ~2400 units from its seed, so the seed *is* the layout — forces only fine-tune; (b) lanes are `HORIZONTAL_SPACING` (80) apart while node widths reach ~496, so adjacent lanes cannot hold wide nodes without overlap (impossible geometry, not tuning). And one structural fact: banner groups are **contiguous** slice ranges, so patch-order lanes already make each group a contiguous run of lanes — cluster visibility comes from lane *ordering*, not from a force.

Therefore the sections seed is:
1. **Lane per circuit** in patch order (`slice_index`: circuits → `section_index`; controllers/input-jacks → left outer lane; output jacks → right outer lane).
2. **Width-aware lane width**: `lane_w[s] = max(estimated_widths[i].min(SECTIONS_LANE_MAX_NODE_W))` over the lane's members — nodes narrower than the cap (`SECTIONS_LANE_MAX_NODE_W = 240`) no longer inflate lanes, and the cap is scoped to sections mode so column mode's `estimated_widths` and layouts are untouched.
3. **Rows**: lanes are packed left→right into rows and wrap when `row_x + lane_w[s]` would pass `SECTIONS_ROW_TARGET_W = 4000` (when the row already holds ≥1 lane). A 500-circuit patch therefore reads as a roughly square grid of lanes instead of a 40:1 ribbon.
4. **Slice spring retargeted** to the node's lane slot `(slot_x, slot_y)` in *both* axes — this is what holds the wrapped structure together while staying weak enough that cables can still drag nodes toward the border between lanes (the user's stated intent).
5. **Section-hole anchor retargeted** to the mean lane slot `(slot_x, slot_y)` of the group's members, so groups compact inside the wrapped grid; still fixed once per solve.

Unchanged: cable springs, size-aware repulsion, force mode (byte-identical golden vectors), and the determinism contract. The user chose "chain, then wrap" and "cap node width at overview zoom" over the alternatives; the cap is applied to lane sizing *and* to the drawn frame width **in sections mode only**, because the layout is solved at build time and zoom is a render-time concern (an unconditional global cap would have changed the default column layout).

### D2 — Fixed section holes from patch order, not live centroid

Current force solver uses `COHESION_K` pulling toward live centroid of members (chases positions). Sections mode computes holes **once** at solve start:

```rust
let hole_for_group: Vec<Option<(f32,f32)>> = groups.iter().map(|g| {
    let mean_slice = g.member_indices.iter().map(|&i| slice_index(i)).sum::<usize>() as f32 / g.member_indices.len() as f32;
    Some((mean_slice * HORIZONTAL_SPACING, 0.0)) // y = 0, members distribute vertically via repulsion
}).collect();
```

**Why**: "Black holes" metaphor — sections anchor, don't chase. Deterministic, stable clusters visible as density (Sternenhaufen).

**Alternatives**: Live centroid — regresses to current weak cohesion behavior.

### D3 — Size-aware repulsion using `node_world_sizes`

Current repulsion uses fixed `REPULSION_RADIUS=120` blind to node size. Sections mode reads `node_world_sizes(graph)` (existing function returning `(width, height)` per node) and repels proportional to `max(node_width, node_height)`:

```rust
let effective_radius = REPULSION_RADIUS + 0.5 * (sizes[i].0.max(sizes[i].1));
```

**Why**: Overlap prevention in physics mode without hard columns. Near-linear grid hashing unchanged.

### D4 — Sliders panel as a small egui overlay in the graph pane

New `ui::graph::paint_sliders_panel(ui, app, graph_rect)` paints a small panel at top-right of the graph pane (toggleable, key `S`). Three `egui::Slider` rows bound to `App.slice_force`, `App.section_force`, `App.tension`. Each drag calls `rebuild_graph` → `solve_graph_positions` → `fit_graph_camera`.

**Pointer priority**: panel rect consumes mouse before node-drag/marquee/minimap (same pattern as minimap click).

**Why**: egui-native, no new window, keyboard fallback remains.

### D5 — Arrange button re-runs from current positions (anneal)

Button in sliders panel calls `run_iterations` from `App.graph_positions` (not `seed_positions`) with full iteration budget and current force constants. Freezes at energy threshold. Reports `Arranged (sections)`.

**Why**: Anneals from current state instead of reseeding — useful after manual drags or slider tuning.

### D6 — Centering uses live canvas in sections mode

`c`/`Shift+c` already use `graph_canvas_px` published per frame. Sections mode ensures `graph_canvas_px` is published (already done by `paint_scene_in` for all modes). No change needed — just verified by scenario.

### D6 — PatchOrder within-column ordering for column layout

Add `LayoutOrdering::PatchOrder` to the existing `LayoutOrdering` enum (`src/config.rs`, `src/layout.rs`). The column assignment stays topological (dense-normalized depth), but within-column ordering uses patch order (file index) instead of strict file order or barycenter. This keeps banner-group members vertically contiguous while preserving signal-flow columns.

**Why**: Low-risk addition to the column path; reuses existing column machinery; sections become vertically contiguous without switching to physics mode.

**Alternatives**: None needed — this is a pure addition to the existing ordering enum.

### D7 — PatchOrder persists in `[layout] ordering`

Config key `[layout] ordering = "patch"` selects it. `Column-Strict`/`Column-Barycenter`/`Column-PatchOrder` cycle via `Alt+[`/`Alt+]` in column mode (or new key).

## Risks / Trade-offs

- [Slice stiffness too high/low] → Members lock to columns or smear. Mitigation: default `SLICE_K` tuned so cable tension can overcome it; slider for tuning.
- [Hole strength too high/low] → Clusters collapse or scatter. Mitigation: default `SECTION_HOLE_K` tuned; keyboard step if needed.
- [Wide graphs still fail to center] → Already a fit issue; separate regression test added.
- [Sliders panel fights node drag] → Panel rect consumes pointer first; panel toggle key `S` hides it when dragging.
- [Determinism across slider values] → Contract: same patch + same force triple → same layout. Verified by test.
- [PatchOrder increases crossings vs Barycenter] → Trade-off: sections contiguous vs crossing count. User chooses.

## Migration Plan

None needed — new mode + new ordering option, column remains default, no config migration, no data migration.

## Open Questions

- Should `SECTION_HOLE_K` get a keyboard step like tension? (Start without; add if tuning painful)
- Should sliders persist across patch loads? (Start without; tension doesn't either)