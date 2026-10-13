# Proposal

## Why

The signal-flow graph view fails to scale to real DROID patches. With hundreds of circuits, the current column arrangement produces a horizontally wide, rigid layout that cannot be centered or navigated meaningfully. The retained force-directed solver is unreadable at patch scale (the original reason it was demoted from default). Sections — the banner-group clusters that represent functional module groups — are invisible in both modes: the column layout places members in dense columns without visible grouping, and the force solver's centroid cohesion is too weak and chases live positions instead of anchoring.

The user wants a third layout mode where sections act as "black holes" — invisible fixed anchors computed from patch order that attract their member circuits with soft springs, while cable springs pull across sections and size-aware repulsion prevents overlap. Sliders for slice/section/cable forces and an Arrange button to re-run the joint solve complete the tuning surface.

## What Changes

- **New layout mode `sections`** alongside `column` and `force` in `[layout] mode`, selectable via the existing `h` toggle and `a` cycle.
- **Joint black-hole solve**: one `run_iterations` pass with four forces — slice springs (patch-order slots), section-hole springs (fixed anchors at mean slot of banner-group members), cable springs (bundle-normalized), and size-aware grid-hashed repulsion. Cluster boxes off in this mode.
- **Slice stiffness** constant + keyboard step, cable tension reused, section-hole strength constant.
- **Sliders panel** in the graph pane (top corner, toggleable): slice force, section force, cable force — draggable `egui::Slider` widgets that re-solve live.
- **Arrange button** beside sliders: re-runs the joint solve from the current positions (anneal), then freezes.
- **Centering fix**: first-frame fit and `c`/`Shift+c` re-fit work against the published canvas in sections mode.
- **New `LayoutOrdering::PatchOrder`** for column mode: columns from signal-flow topology, within-column order follows patch order — keeps sections contiguous while preserving left→right signal flow. Selectable via `[layout] ordering = "patch"`.

### As-built findings (fixes the implementation had to add)

- **Size-aware repulsion was inert.** As first written it scaled force magnitude by `1/d²` *independent* of the node's size, and kept the repulsion grid cell at the fixed `REPULSION_RADIUS` (120) while node widths reach ~496 — so wide pairs were never even considered, and two 496-wide nodes settled ~196 apart (overlap ~300). Fixed to a linear penetration ramp (`mag` ∝ overlap, exactly zero at the required separation) with a graph-adaptive grid cell. Force mode is byte-identical (golden-vector test).
- **The sections seed exploded on big cyclic patches.** Sections mode initially reused the force-mode Sugiyama seed, `x = topological_depth * HORIZONTAL_SPACING`, but `topological_depth` is an `n`-pass Bellman-Ford whose depth on cyclic graphs reaches ~cycle-length × n. On `fixtures/own_scale.ini` (511 nodes) that seeds a **19,621,280**-wide layout, which the clamped integrator (≤2400 total travel) cannot undo. Sections mode now seeds by **patch order** per D1: `own_scale` is **39,540 × 60,020** (falsified both ways; scale guard test added).
- **The same seed pathology still breaks the retained force mode** (`own_scale` → 19,621,300) — pre-existing, out of scope here, filed as bead `droid_tui-68g`.
- **Open design item: lane pitch vs node width.** Lanes are `HORIZONTAL_SPACING` (80) apart while node widths reach ~496, so adjacent lanes cannot hold wide nodes without overlap — geometry, not tuning. Being resolved before the change closes; see `design.md` D1.

## Capabilities

### New Capabilities

- `graph-sections-layout`: sections-mode joint black-hole solver with live force sliders and Arrange button

### Modified Capabilities

- `graph-arrange`: the `h`/`a` cycle and `[layout] mode` gain a third `sections` entry
- `signal-flow-graph`: sections mode replaces the retained force solver as the physics-based option

## Impact

- Touched: `src/layout.rs` (sections-mode joint solve + constants + sliders integration), `src/graph.rs` (section-hole anchor computation from banner groups), `src/app.rs` (sections-mode state + sliders panel + Arrange handler), `src/gui/graph.rs` (sliders panel painting + Arrange button + pointer priority), `src/config.rs` (slice/section constants), `src/theme.rs` (slider tokens if needed).
- Behaviour: `g g` opens in sections mode when selected; `h`/`a` cycle includes it; sliders re-solve live; Arrange button re-solves from current positions.
- No `.ini` mutation, no config key beyond the new `[layout] mode = "sections"`, no persistence beyond session.
- Closes the "big patches unreadable" gap; column remains default.

## Non-goals

- No container rectangles or visible section boxes (sections are invisible anchors per the black-hole metaphor).
- No two-phase container packing (sections are not layout containers).
- No RNG or non-determinism (fixed holes from patch order + RNG-free forces).
- No up/down arrange direction (left→right only; direction is a later `[layout] direction` key).
- No section-hole strength slider initially — fixed constant, keyboard-adjustable if tuning proves painful.
- No persistence of slider values across patch loads (session only, like tension today).