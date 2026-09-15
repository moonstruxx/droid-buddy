## 1. Subset builder

- [x] 1.1 Add pure induced-subgraph builder on `Graph` filtering nodes to an influenced set, keeping edges with both endpoints inside and inheriting intersecting banner clusters, with unit tests for membership, cluster inheritance, and determinism <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/graph.rs] -->
- [x] 1.2 Add subset unit tests for edge cases (empty set, single node, 2-node chain, register-edge exclusion) and verify `cargo test graph` passes <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [src/graph.rs] -->

## 2. App state and solve

- [x] 2.1 Add influence-subset state fields plus `apply_influence_subset` (subset-relative pins, `layout::solve`, positions mapped back) and a toggle, mirroring `apply_dependency_subset` <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs] -->
- [x] 2.2 Re-apply the influence subset in `rebuild_graph` next to the dependency re-apply with the same root-vanished clearing rule, and add state/model tests <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/app.rs] -->

## 3. Render and input

- [x] 3.1 Draw only the influence subset with its own camera fit when the filtered view is active, keeping the FULL highlight path unchanged, with headless shape tests <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/gui/graph.rs] -->
- [x] 3.2 Wire the filtered view toggle key on the graph surface (no collision with `f`/`x`/`p`) with handler tests for toggle, clear, and status text <!-- agent: rusty-engineer.build, depends_on: [2.2, 3.1], touches: [src/handler.rs] -->

## 4. Verification

- [x] 4.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked` and fix all failures <!-- agent: horst-engineer.fast, depends_on: [1.2, 2.2, 3.1, 3.2], touches: [] -->