## Design Notes

### Influence walk

- Pure function `Patch::influence_subtree(root_vars: &[String]) -> InfluenceSubtree { nodes, edges }` over `cable_index` (HashMap<String, CableIndexEntry>) + new `circuit_outputs: Vec<Vec<String>>` parallel to `sections` (or `HashMap<NodeId, Vec<String>>`). Iterate cable entries sorted by name; sink refs sorted by (section_name, param_key) for determinism (D9). Queue = VecDeque<String>. visited_cables: HashSet<String>, visited_nodes: HashSet<NodeId>.
- Hop rule (from user feedback): `sink circuit has input AND output ports AND is on current flow` → check `circuit_outputs[sink_idx].is_empty() == false` (input presence implied by being a sink of the current cable; no name allowlist).
- HW -> VAR derivation: scan `sections` for param values containing the hw token (reuse boundary-aware scanner) and `output = _VAR` in same section; root_vars = those `_VAR`.

### Graph filtered subgraph

- `Graph::filtered_influence(&self, subtree: &InfluenceSubtree) -> Graph` builds an induced subgraph: nodes = those with id in subtree.nodes, edges = those with cable in subtree.edges and both endpoints in node set, clusters = banner clusters filtered to member ranges intersecting node section_indices. Validation re-runs on the subgraph.

### Quad layout

- `App { showing_quad: bool, quad_focus: QuadFocus (Panels|Source|GraphFull|GraphFiltered), active_modifier_var: Option<String>, influence: Option<InfluenceSubtree>, filtered_graph: Option<Graph>, filtered_positions: Vec<(f32,f32)> }`.
- `ui::render` branch: `if showing_quad { render_quad } else { // existing }`. `render_quad` does vertical split `header(3) / body / status(3)` where `body = Layout::vertical([top 50%, bottom 50%])`, each row = `Layout::horizontal([50%,50%])`. Top uses existing `render_patch_grouped` and `render_source_*` helpers; bottom uses two invocations of graph rendering with distinct position sets. Below width threshold, fall back to `render_embedded_main` or `render_graph` single.
- Theme tokens map to existing palette generation; no new color literals outside `theme.rs`.

### Kitty-gfx (feature flag)

- `Cargo.toml` feature `kitty-gfx = []`. `ui::render_graph_edges/nodes` branch on `cfg(feature = "kitty-gfx")` + runtime `is_kitty()` (checks `KITTY_WINDOW_ID`/`KITTY_LISTEN_ON` or `TERM == "xterm-kitty"`). Fallback always box-drawing. No IPC.

