# Tasks

## 1. Bound the layered force seed

- [ ] 1.1 Dense-normalize the layered depth inside `seed_positions` in `src/layout.rs` (`x = dense_columns(&topological_depth(graph))[i] * HORIZONTAL_SPACING`), passing the dense depth on to `crossing_minimized_orders` so layers and within-layer orders are unchanged. Verify `cargo test layout::` is green except expectations that pinned sparse/cyclic depths. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/layout.rs] -->
- [ ] 1.2 Add the scale-guard unit test in `src/layout.rs`: on the real cyclic `fixtures/own_scale.ini` graph, assert the force seed's horizontal extent is bounded by `(distinct layers − 1) × HORIZONTAL_SPACING × 1.5` and the solved force-mode bbox stays proportional to patch size — not the ~19.6M smear. Falsify by reverting 1.1 and showing the test fails with the raw-depth number. <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [src/layout.rs] -->
- [ ] 1.3 Update every existing expectation that pinned force-mode positions on a sparse or cycle-inflated depth, to the new contract: unit tests in `src/layout.rs`, force-position assertions in `src/regression.rs` and `src/app.rs`. For each, report the old and new values and state the contract in a comment. Keep at least one contiguous-depth golden vector untouched to prove the identity case. <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [src/layout.rs, src/regression.rs, src/app.rs] -->

## 2. Verify and close

- [ ] 2.1 Run the full gate: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`, `tools/screenshots/render.sh` — every command exits 0. Confirm the screenshot set is unchanged (this change alters force-mode positions, so any force/sections shot that moves must be explained and either justified or shown not to be reachable from the harness shots). <!-- agent: horst-engineer.fast, depends_on: [1.1, 1.2, 1.3], touches: [] -->
- [ ] 2.2 Close bead `droid_tui-68g` with the change reference, after the gate is green. Verify with `bd show`. <!-- agent: devops-engineer.fast, depends_on: [2.1], touches: [] -->
