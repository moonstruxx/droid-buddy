## 1. Solver — column path

- [x] 1.1 Add the column-placement function to `src/layout.rs` (shared capped Bellman-Ford depth, dense-normalized columns `0..N-1`, width-aware grid placement with per-column max width, centered blocks, vertical stacking, `GRID_SNAP` snap, no force relaxation) and verify a determinism test (same graph twice → identical positions) and a no-overlap property test pass <!-- agent: api-engineer.build, depends_on: [], touches: [src/layout.rs] -->
- [x] 1.2 Add the within-layer ordering switch (strict slot order default, barycenter crossing-minimization sweeps option) to the column path and verify tests cover both orderings with identical column assignment <!-- agent: api-engineer.build, depends_on: [1.1], touches: [src/layout.rs] -->
- [x] 1.3 Add the node-size estimator inside the solver (circuit name length + port label lengths, same inputs the renderer uses) feeding the grid placement and verify the no-overlap test holds against the estimated widths <!-- agent: api-engineer.build, depends_on: [1.1], touches: [src/layout.rs] -->

## 2. Config & state

- [x] 2.1 Add `[layout] mode` (default `column`) and `[layout] ordering` (default `strict`) to `src/config.rs` with XDG load/save, warn-once on malformed values, fallback to defaults, and verify config tests cover missing table, force mode, barycenter ordering, and malformed fallback <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/config.rs] -->
- [x] 2.2 Add the layout-mode state to `App` (seeded from config at startup, `solve` dispatch by mode) and verify an app-level test that the column mode is the default after `load_patch` <!-- agent: api-engineer.build, depends_on: [1.1, 2.1], touches: [src/app.rs] -->
- [x] 2.3 Place controller/jack nodes in fixed outer columns on the column path (controllers left, inputs left, outputs right, circuit columns between) and verify a layout test asserting the outer-column placement <!-- agent: api-engineer.build, depends_on: [1.1], touches: [src/layout.rs] -->

## 3. Interaction

- [x] 3.1 Add the layout-mode toggle keybinding and status hint (column ↔ force, respecting the keybinding spec) and verify handler tests cover the toggle on both arrangements <!-- agent: rusty-engineer.build, depends_on: [2.2], touches: [src/handler.rs] -->
- [x] 3.2 Wire pin semantics on the column path (fixed-position anchors; force path unchanged) and verify a layout test that a pinned node keeps its position while the rest arrange around it <!-- agent: api-engineer.build, depends_on: [2.3, 3.1], touches: [src/layout.rs, src/app.rs] -->

## 4. Tests

- [x] 4.1 Add column-layout unit tests (column assignment, dense normalization, no-overlap property, grid snap, determinism, strict vs barycenter ordering) and verify the full `layout.rs` test module passes <!-- agent: horst-engineer.build, depends_on: [1.2, 1.3], touches: [src/layout.rs] -->
- [x] 4.2 Update snapshot and drag-interaction tests for the new default arrangement (regenerate insta snapshots with `cargo insta accept --include-ignored`, keep force-path drag tests) and verify the snapshot suite passes <!-- agent: horst-engineer.build, depends_on: [3.2], touches: [src/regression.rs, src/snapshots/**, src/handler.rs] -->
- [x] 4.3 Run the full gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo insta test --check`, `cargo test --release --test perf_gate --locked`, `cargo build --release --locked`) and verify all exit 0 <!-- agent: horst-engineer.fast, depends_on: [4.2], touches: [] -->

## 5. Docs

- [x] 5.1 Update ARCHITECTURE.md and DESIGN.md for the column arrangement, the `[layout]` config table, and the toggle, and verify the docs reflect the new default and the retained force path <!-- agent: rusty-engineer.fast, depends_on: [4.3], touches: [ARCHITECTURE.md, DESIGN.md] -->