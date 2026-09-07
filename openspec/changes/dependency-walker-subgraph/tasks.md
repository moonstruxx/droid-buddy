## 1. Upstream dependency walk

- [x] 1.1 Add `Graph::upstream_dependencies(&self, root: NodeId) -> Vec<NodeId>`: reversed-edge BFS over `edges` (sink -> sources), cycle-safe via a visited set, stopping at controller and input-jack nodes; a controller or input-jack root yields just itself. Add a subset helper returning the edges whose endpoints are both in a node set. Verify: unit tests cover a linear chain, a fork (two producers of one input), a cycle (visited once), a controller leaf (walk stops, LED-write edges are not followed), and the root-is-leaf degenerate case. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/graph.rs] -->

## 2. Filter state and the `f` key

- [ ] 2.1 Add `App.dependency_root: Option<NodeId>` reset on `load_patch` and graph close. On the graph pane, `f` engages the filter rooted at the hovered node (fallback: `selected_circuit`; neither -> no-op with a status hint), freezing the root; a second `f`, Esc, patch load, and graph close clear it. Status: `Dependencies of <label>: N nodes`. Verify: handler unit tests assert engage/clear transitions and the fallback chain. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/app.rs, src/handler.rs] -->

## 3. Filtered rendering

- [ ] 3.1 When `dependency_root` is set, render only the dependency set and its internal edges through the existing graph-slot machinery, solved deterministically as a subset (positions parallel to the subset). Verify: a rendering snapshot shows the filtered view for an output-jack root on a fixture with producers and an unrelated branch, with the unrelated branch absent. <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/ui.rs] -->

## 4. Regression and full gate

- [ ] 4.1 Add a graph-level regression fixture: an output jack fed by two producers (one through a cable, one direct register), plus an unrelated circuit, and assert the dependency set excludes the unrelated circuit and the walk stops at the controller feeding a LED-write branch. Verify: `cargo test` passes. <!-- agent: horst-engineer.build, depends_on: [3.1], touches: [src/graph.rs, fixtures/] -->
- [ ] 4.2 Run the full verification gate and accept any intended snapshot changes. Verify: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, and `cargo build --release --locked` all exit 0. <!-- agent: horst-engineer.fast, depends_on: [4.1], touches: [] -->