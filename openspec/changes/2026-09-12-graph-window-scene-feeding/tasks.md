## 1. Feed the graph scene into the window each frame

- [x] 1.1 Build the `SceneSpec` per frame in the main.rs window path from the App's live graph state — the same token classification the terminal kitty path already performs (`NodeTokenSpec`/`EdgeTokenSpec`/`ClusterTokenSpec` from ui.rs) — through `build_scene_spec` with the active theme, and call `GraphWindow::set_scene` each frame alongside `fill_placeholder` in the multiplexed loop. Opening the window (`g w`) with a patch loaded must show the same nodes, edges, and clusters the terminal tile renders for the same patch (same `SceneSpec`). Verify: `cargo test --features gui` compiles and the gui-gated unit/snapshot tests pass. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/main.rs, src/gui.rs, src/graph_render.rs, src/ui.rs] -->

## 2. Window scene tracks the live graph

- [x] 2.1 The window scene follows graph rebuilds (patch load, open/close graph, per-circuit disable, pin/unpin, select-state cycles, tension changes), camera pan/zoom (the `WindowFrame` interaction report already mutates `App.graph_camera`), and theme changes — the per-frame `set_scene` must re-derive the spec from current `App` state so no rebuild/pan/theme event leaves the window stale. Verify: gui-gated tests assert the spec re-derivation inputs (graph, positions, camera, theme) are read fresh each frame; manual path unchanged. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/main.rs, src/gui.rs] -->

## 3. Regression proof pins what the window paints

- [x] 3.1 Add a headless shape-level regression proof: `window_scene_spec_tests::scene_carries_identity_ports_and_edges` builds a `SceneSpec` from live App state (two nodes + one edge), pins node identity + `input_port`/`output_port` flags and edge wiring via a gui-gated unit test — the window paints nodes, edges, and clusters, not just the clear color. Verify: `cargo test --features gui` passes with `INSTA_UPDATE=no` (981 pass, 952 default). <!-- agent: horst-engineer.build, depends_on: [2.1], touches: [src/gui.rs, src/graph_render.rs, src/ui.rs] -->

## 4. Full verification gate

- [x] 4.1 Run the full verification gate and accept any intended snapshot changes. Verify: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test` (952 pass) / `cargo test --features gui` (981 pass), and `cargo build --release --locked` all exit 0. <!-- agent: horst-engineer.fast, depends_on: [3.1], touches: [] -->