# Tasks: patch-diff-viewer

## 1. Model — Order-Independent Structural Diff

- [x] 1.1 Implement `src/diff.rs`: pure `DiffReport` (`added_cables`, `removed_cables`, `changed_cables`, `added_nodes`, `removed_nodes`, `changed_nodes`) and `fn diff_patches(&Patch, &Patch) -> DiffReport` keyed by `HwComponent.id`, `NodeId=(circuit,instance_index)`, and cable name; wiring via `CableIndexEntry.sources` (set) + `sink_refs` (set of `(sink_circuit, sink_param)` resolved to NodeId); settings as per-NodeId `HashMap<param_key,param_value>`; deterministic sorted output; wire into `src/lib.rs` <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/diff.rs, src/lib.rs] -->
- [ ] 1.2 Add `src/diff.rs#tests` + fixtures: reordered panels → equal; added/removed circuit; cable added; cable sink changed; changed param value; param reorder → equal; deterministic (order-independent) <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [src/diff.rs#tests] -->

## 2. App Wiring + Picker

- [ ] 2.1 Add `App.diff_patch`/`diff_report`/`diff_scope`, `load_diff_patch(path)` (parse via `Patch::from_ini_file`, compute report, emit event), `clear_diff()` on `load_patch`; add `Event::DiffComputed` to `events.rs`; `g d` opens picker for B patch in `handler.rs`; `d` toggles diff overlay; `Esc` clears scope then overlay; `diff_scope` from `App.selected_component` <!-- agent: api-engineer.build, depends_on: [1.1], touches: [src/app.rs, src/events.rs, src/handler.rs] -->

## 3. Graph + Source Highlighting

- [ ] 3.1 Add `theme.rs` tokens `graph_edge_diff_added`/`graph_edge_diff_removed` wired into `classic`/`terminal`/`mono`; extend `ui.rs:render_graph` `cable_color()` precedence to `error` > `diff` > modifier > `CableKind`; node-title marker for changed `NodeId` params; cluster tint when all members added/removed; source-pane per-param `+`/`-` markers; add snapshot matrix (`classic`/`terminal`/`mono` × widths) in `regression.rs` <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/theme.rs, src/ui.rs, src/regression.rs] -->

## 4. Scoped Filter

- [ ] 4.1 Implement `scope_report(report, token, patch)` filter in `src/diff.rs` (retain cables/params intersecting selected token's influence); status hint `Diff scope: <token> (N cables)`; verify `cargo test` + `cargo clippy` green <!-- agent: api-engineer.build, depends_on: [2.1], touches: [src/diff.rs, src/handler.rs] -->
