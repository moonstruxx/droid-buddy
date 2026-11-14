# Tasks

## 1. Palette contrast

- [x] 1.1 Widen `rendermetrics`'s WCAG helpers to `pub(crate)` and add `theme::tests::cluster_title_meets_text_contrast_in_every_palette`, enumerating `THEMES`, resolving each palette's `graph_cluster_title` against its `graph_canvas_bg` through `Theme::rgb`, skipping the `terminal` pair whose tokens are `Reset`, and asserting at least 4.5:1; verify it fails on today's `classic` palette (2.44:1) and that `classic`/`mono` pass after 1.2 <!-- agent: frontend-engineer, depends_on: [], touches: [src/theme.rs, src/rendermetrics.rs] -->
- [x] 1.2 Set the `classic` palette's `graph_cluster_title` to `Color::LightBlue` and leave `graph_cluster_border` on ANSI Blue; verify `cargo test --lib theme::` passes, including the 1.1 guard and the existing ANSI-16 table tests <!-- agent: frontend-engineer, depends_on: [1.1], touches: [src/theme.rs] -->

## 2. Cluster-title paint rule

- [x] 2.1 Paint cluster titles at the fixed screen-space `CLUSTER_TITLE_PX` size instead of `CLUSTER_TITLE_PX × zoom`, replacing the `MIN_LABEL_PX` omission gate with a box-room gate measured on the already-projected cluster rect (enough height for one title line plus its inset, enough width for a readable fragment of the title), and delete the now-unread `MIN_LABEL_PX` (the port-marker threshold is `LOD_MIN_NODE_H`); verify the graph paint tests and `cargo test --lib gui::graph` still pass <!-- agent: frontend-engineer, depends_on: [], touches: [src/gui/graph.rs] -->
- [x] 2.2 Add paint-level guards through a plain `egui::Context`: the painted cluster-title font size is identical at two camera zooms, a title paints at a low zoom whose zoom-derived size would previously have omitted it, and a cluster box too small to hold a title paints none; verify the new tests fail against the pre-2.1 painter behaviour and pass after it <!-- agent: frontend-engineer, depends_on: [2.1], touches: [src/gui/graph.rs] -->

## 3. Documentation and gates

- [x] 3.1 Update `docs/views.md`'s cluster-container sentence to state that titles are screen-space constant and yield to boxes without room, then regenerate `ARCHITECTURE.md` (and its `§15` open bullet and ADR/footer record) with the project's architecture skill; verify `ARCHITECTURE.md` no longer lists the cluster-title contrast item as unfixed <!-- agent: devops-engineer.build, depends_on: [1.2, 2.2], touches: [docs/views.md, ARCHITECTURE.md] -->
- [x] 3.2 Run the repository gates — `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, and `cargo run --release -- --self-test` — and record the exit status of each; verify all four exit 0 <!-- agent: devops-engineer.fast, depends_on: [1.2, 2.2], touches: [] -->

- [x] 3.3 Regenerate `DESIGN.md` (palette token table + cluster bullet) with the project's design skill; verify `grep -n "cluster-title" DESIGN.md` reports the LightBlue value and the diff touches only `DESIGN.md`
