# Design: patch-diff-viewer

## Context

`droid_tui` already parses patches into a `Patch` model with `hw_components`, `sections` (with `instance_index` semantics matching `GraphNode`), and a `cable_index` (`CableIndexEntry { sources, sink_refs }`), and already renders a signal-flow graph with a `cable_color()` precedence (`graph_edge_error` > modifier hue > `CableKind`) from semantic theme tokens. See proposal.md for motivation; the specs define the observable behavior this design implements.

The gap: no way to compare two patches. A raw-line diff is noise because HW module order varies. This change adds a pure structural diff and surfaces it in the graph view.

## Goals / Non-Goals

**Goals:**
- Order-independent structural diff (wiring + settings) as a pure, deterministic pass.
- Patch-wide and component-scoped diff via a filter on the report.
- Visual highlighting in the graph and source panes via new semantic edge tokens.
- Consistent with the layered monolith (pure model, renderer-owned layout, sync event bus).

**Non-Goals:**
- No three-way merge / apply / write-back.
- No persistence of diff state across sessions.
- No diff of the DROID circuit schema manifest — patch content only.

## Decisions

### D1: Pure `src/diff.rs` module — diff on the parsed `Patch`, keyed not positioned
A new module with `struct DiffReport { added_cables, removed_cables, changed_cables: Vec<ChangedCable>, added_nodes, removed_nodes, changed_nodes: Vec<ChangedNode> }` and `fn diff_patches(a: &Patch, b: &Patch) -> DiffReport`. Comparison is by key:
- `hw_components` keyed by `HwComponent.id` (`B1.1`, ...)
- sections keyed by `NodeId = (circuit_name, instance_index)` — identical to `GraphNode` identity in `graph.rs`, so same circuits in different file order compare equal
- `cable_index` already `HashMap` keyed by cable name; compare `sources` as a set and `sink_refs` as a set of `(sink_circuit, sink_param)` after resolving `section_name → NodeId`
- settings as per-`NodeId` `HashMap<param_key, param_value>` (value-string equality)

Determinism: sort all report collections (cables, nodes, param keys) so the diff is reproducible regardless of `HashMap` iteration order — same rationale as `Graph::build_from_patch` edge sorting. `banner_groups` are excluded (display grouping only).

Alternatives considered: diffing raw lines (rejected — order noise), diffing serialized JSON (rejected — loses instance/preamble identity and adds a chunk-format dependency). Keyed struct comparison on the parsed model wins on precision and code reuse.

### D2: Diff state lives on `App` alongside the loaded patch
`App` gains `diff_patch: Option<Patch>`, `diff_report: Option<DiffReport>`, and `diff_scope: Option<String>` (token or `None` = patch-wide). `load_diff_patch(path)` parses via `Patch::from_ini_file`, computes `diff_report = diff_patches(&patch, &diff_patch)`, and emits `Event::DiffComputed`. `clear_diff()` clears all three and is called on `load_patch` (same reset pattern as `processing_paused`/`disabled_circuits`). No influence caching — the report is recomputed on `load_diff_patch`, cheap for real DROID patch sizes.

### D3: Event-bus extension — `Event::DiffComputed`
Add `Event::DiffComputed` to `events.rs`, dispatched after `diff_patches`. Consistent with the existing synchronous bus (`GraphRebuilt`/`NodeMoved`/`TopologyError`); a future status-bar subscriber reads counts. No production subscriber today (matches the D6 extension-point posture).

### D4: Scoped mode is a filter on `DiffReport`, not a second diff pass
`diff.rs` exposes `fn scope_report(report: &DiffReport, token: &str, patch: &Patch) -> DiffReport` that retains only cables/params whose sections' HW tokens intersect the token's influence (reusing the `hw_token_to_vars`/influence pattern). `App.selected_component` drives it; `Esc` clears scope before closing the diff overlay. UI reuses the existing `ViewerFocus` precedence so panels/source interact normally while diffed.

### D5: Two new semantic edge tokens + graph styling
`theme.rs` gains `graph_edge_diff_added` and `graph_edge_diff_removed` (added vs removed color, distinct from `graph_edge_error`), wired into `classic`/`terminal`/`mono` palettes (terminal/mono → distinct grays). `ui.rs:render_graph` extends `cable_color()` precedence to `error` > `diff` > modifier > `CableKind`; node titles get a marker suffix when the `NodeId`'s params differ; cluster containers get a tint when all members are added/removed. The disabled-circuit dim path (`graph_node_dim`) stays separate. Source pane reuses the modifier highlight hue for per-param `+`/`-` markers.

### D6: `g d` picker flow follows the existing `g`-prefix convention
`g d` opens the picker (reusing `showing_picker` for `.ini` selection) to load the B patch; `d` toggles the diff overlay while `diff_patch.is_some()`; `Esc` clears `diff_scope` first, then the diff overlay. Consistent with `g v` (source) / `g g` (graph).

### D7: Dependency direction preserved
`diff.rs` depends on `patch.rs`/`graph.rs` (pure); `app.rs` on `diff`; `ui.rs`/`handler.rs` on `app`/`diff`. No cycle, no terminal dep in the model.
