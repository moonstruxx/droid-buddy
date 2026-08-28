# Exploration: Patch Diff Viewer (order-independent wiring + settings)

**Goal:** Compare two DROID `.ini` patches ignoring HW module order, with patch-wide and per-component scopes, and highlight differences in the signal-flow graph.

## 1. Order-independent diffing

- Diff on **parsed `Patch` model** (`src/patch.rs`), not raw lines.
- `Patch` fields that are order-sensitive today: `hw_components: Vec<HwComponent>`, `sections: Vec<IniSection>`, `modules`, `cable_index: HashMap<String, CableIndexEntry>`, `banner_groups`.
- Canonicalize by **key, not position**: `hw_components` keyed by `HwComponent.id` (e.g. `B1.1`), `sections` keyed by `(circuit_name, instance_index)` = `NodeId` (`src/graph.rs`), `cable_index` already a map keyed by cable name (`_NAME`); `banner_groups` ignored for diff (display grouping only).
- Identity of repeated sections uses `instance_index` (file-order index within same circuit name) — identical to `GraphNode { circuit, instance_index, section_index }` in `graph.rs`. Two patches with same circuits in different file order compare as equal.

## 2. Wiring comparison

- Source of truth: `Patch.cable_index` (`src/patch.rs:collect_cable_index`).
  - `CableIndexEntry { sources: Vec<String>, sink_refs: Vec<(String,String)> }`
  - `sources` = circuits with `output = _CABLE`; `sink_refs` = ordered `(section_name, param_key)` where `param_value == _CABLE` (e.g. `input = _CABLE`). Preamble cable maps excluded.
- Per-cable equality: `sources` as set (order-insensitive) + `sink_refs` as set of `(sink_circuit, sink_param)` after resolving `section_name → NodeId`. Cables present in one patch only = added/removed. Same cable with different sink set = changed.
- LED association (`src/patch.rs` LED-association detection: bare `led = L.N` or `ledN = L.M` paired by suffix) is part of wiring; compare `HwComponent.led` alongside cable_index.

## 3. Settings comparison

- Non-wiring params: every `[section]` key/value where value is not a `_CABLE`.
- Key each section by `NodeId = (circuit, instance_index)`; within a section compare `HashMap<param_key, param_value>` (value string equality). Param order inside a section is irrelevant.
- Added/removed circuits = whole `NodeId` present in one patch only. Changed = same `NodeId` with differing param values (report per-key diff).

## 4. Scoped mode (patch-wide or selected HW component)

- **Patch-wide (default):** diff all of `cable_index` + all `NodeId` params.
- **Component-scoped:** filter to bindings/params involving a selected hardware token.
  - Reuse existing selection infra: `App.selected_component: Option<String>` (`src/app.rs`), `hovered_component`, `component_rects` hit-test (`src/handler.rs` → `src/ui.rs:component_rects`).
  - Token → vars → influence pattern already exists (`Patch.hw_token_to_vars` → `influence_subtree`); for diff, filter `sink_refs`/`sources` whose section's HW tokens intersect the selected token's influence, plus direct `HwComponent` entry for that token.
  - UI: when `selected_component.is_some()`, status hint shows `Diff scope: B1.1 (3 cables)`; `Esc` clears scope; same focus rules as source viewer (`ViewerFocus`).
- Scoping is a filter on the already-computed `DiffReport`, not a separate diff pass.

## 5. Visual highlighting in the graph

- The signal-flow graph (`src/ui.rs:render_graph`, opened with `g g`) already has `cable_color()` precedence: `graph_edge_error` (red, `TopologyIssue`) > modifier hue > `CableKind` (`control`/`audio`/`midi`/`unknown` from `theme.rs:Theme { graph_edge_control, ... }`).
- New diff state needs two edge tokens alongside error: `graph_edge_diff_added` + `graph_edge_diff_removed` (e.g. green vs red, distinct from `graph_edge_error`). Precedence: `error` > `diff` > `modifier` > `CableKind`.
- Node highlighting: title marker for circuits whose params differ (e.g. `circuit_name*` or `●` suffix), using existing `graph_node_border`/`graph_node_title` tokens; disabled-circuit dim path (`graph_node_dim`) stays separate.
- Cluster containers (`Cluster { title, section_range }`) get a subtle diff tint when all members are added/removed (reuse `graph_cluster_border` variant).
- Theme addition: two new `Theme` fields, wired in `src/theme.rs` classic/terminal/mono palettes (terminal/mono map to distinct grays).

## 6. Integration sketch (layered monolith)

- **Model (pure, no terminal dep):** new `src/diff.rs` with `DiffReport { added_cables, removed_cables, changed_cables, added_nodes, removed_nodes, changed_nodes }` + `fn diff_patches(a: &Patch, b: &Patch) -> DiffReport` (pure, deterministic, sorted for reproducibility — same rationale as `Graph::build_from_patch` edge sorting).
- **App state (`src/app.rs`):** `diff_patch: Option<Patch>` (second patch), `diff_report: Option<DiffReport>`, `diff_scope: Option<String>` (token or `None` = patch-wide). `load_diff_patch(path)` parses via `Patch::from_ini_file`, computes `diff_report`, emits event. `clear_diff()` on `load_patch` (same reset pattern as `processing_paused`/`disabled_circuits`).
- **Event bus (`src/events.rs`):** new `Event::DiffComputed { added, removed, changed }` (or `DiffUpdated(DiffReport)`), dispatched after `diff_patches`; status bar subscribes (future) to show counts.
- **Keys (`src/handler.rs`):** follow `g`-prefix convention (`g v` source, `g g` graph): `g d` opens picker for B patch; `d` toggles diff overlay while `diff_patch.is_some()`; `Esc` clears diff scope first, then diff overlay. Picker reuse: `showing_picker` already handles `.ini` selection.
- **Renderer (`src/ui.rs`):** `render_graph` already branches on `showing_graph`; when `diff_report.is_some()`, swap `cable_color`/`graph_node_rects` styling to diff tokens. Source pane (`render_source_content`) can show per-param `+`/`-` markers (reuse `selected_component` highlight hue).
- **Dependencies direction preserved:** `diff -> patch/graph` (pure), `app -> diff`, `ui/handler -> app/diff`, no cycle.

## 7. Effort estimate + phasing

- **Phase 1 — Model + tests (S):** `src/diff.rs` pure `diff_patches` + unit tests (order independence, wiring added/removed/changed, settings per-key, heavily modeled on `graph.rs:fixture_tests`). ~1 task, ~120 LOC + fixtures, no UI.
- **Phase 2 — App wiring + picker (S):** `App.diff_patch/diff_report/diff_scope`, `load_diff_patch`, `clear_diff`, `Event::DiffComputed`, `g d` picker flow in `handler.rs`. ~1 task.
- **Phase 3 — Graph highlighting (M):** `theme.rs` two new tokens + palettes, `ui.rs:render_graph` diff-aware `cable_color` + node markers, cluster tint. Snapshot coverage in `regression.rs` (same matrix as `visual-validation`: `classic`/`terminal`/`mono` × widths). ~1 task.
- **Phase 4 — Scoped filter + source markers (S):** `diff_scope` filter in `diff.rs` (view on `DiffReport`), status hint, source pane `+`/`-` markers. ~1 task; can parallelize with Phase 3 (disjoint files: `diff.rs` vs `ui.rs`).

Total: 4 tasks, 2 waves (1→2, then 3‖4). No new deps, no async.
