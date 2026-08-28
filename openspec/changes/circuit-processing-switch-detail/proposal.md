## Why

DROID patches run circuits continuously, but the TUI gives users no way to freeze the simulation or isolate a single circuit while inspecting a patch. And Switch components (S tokens) render with the same white `▣/□` + ON/OFF as buttons, showing no position/state detail and no distinct identity. Users need a global run/pause switch and a per-circuit enable/disable toggle to understand and debug signal flow, plus a Switch cell that actually shows where the switch sits.

## What Changes

- **Global processing pause (`p` key):** `App.processing_enabled` (default true) freezes the simulation — component state mutations (Enter/Space toggles, mouse toggles, knob/fader scroll) are blocked while paused; selection and navigation still work; panels render dimmed with a `PROCESSING PAUSED` status hint; selection-driven influence is cleared while paused.
- **Per-circuit enable/disable (`x` key in graph surface, and quad GraphFull focus):** `Patch.disabled_circuits: HashSet<(String, usize)>` keyed by (circuit name, instance index), with `circuit_disabled()` / `toggle_circuit_processing()`. The structural influence walk dead-ends at disabled sinks (they stay marked influenced, but their outputs do not propagate). In the graph surface, nodes and edges of disabled circuits render dim (`graph_node_dim`/`graph_edge_dim` + DIM, overriding influence highlight). Toggling rebuilds the graph and recomputes influence.
- **Detailed Switch rendering:** new `switch` theme token in all three palettes (classic keeps white so existing snapshots stay byte-identical); Switch cells become independently themeable and render `ComponentState::Value(v)` as `◉ {:.0}%` (positional/attenuator switches, parity with knobs), while baseline `▣ ON` / `□ OFF` rendering is retained.

## Capabilities

### New Capabilities

- `circuit-processing`: global processing pause (`p`) and per-circuit enable/disable (`x` in graph), with blocked state mutation while paused, dimmed panel rendering, dead-end influence propagation at disabled sinks, and dimmed graph nodes/edges for disabled circuits.
- `switch-detail`: detailed Switch component rendering — dedicated `switch` theme token and `Value`-state percentage display alongside the retained `▣ ON` / `□ OFF` baseline.

### Modified Capabilities

- `controller-panels`: add requirement that component cells render dimmed while processing is paused; geometry/hit-testing unchanged.
- `signal-flow-graph`: add requirement that nodes and edges of disabled circuits render dim (overriding influence highlight) and that `x` toggles the hovered circuit while the surface (or quad GraphFull pane) owns input.
- `theming`: add `switch` token to the semantic color-token layer across `classic`/`terminal`/`mono`.
- `keybinding`: add `p` (global processing pause) and `x` (toggle hovered circuit in graph surface / quad GraphFull) keys.

## Impact

- `src/patch.rs` — `disabled_circuits` set + `circuit_disabled`/`toggle_circuit_processing`; influence walk dead-end at disabled sinks.
- `src/app.rs` — `processing_enabled`, `hovered_graph_node` state; `toggle_patch_processing` (clears influence when pausing), `toggle_circuit_processing(node_index)` (toggle + graph rebuild + influence recompute); `recompute_influence` paused-guard.
- `src/handler.rs` — `p` key; `x` key + hover tracking in graph/quad mouse Moved; blocked Enter/Space/mouse-toggle/scroll while paused.
- `src/ui.rs` — panel dim when paused + `PROCESSING PAUSED` status hint; graph disabled node/edge dim; Switch `Value`-percentage rendering.
- `src/theme.rs` — `switch` token in all three palettes.
- Tests: unit tests in patch/app/handler/ui/theme; snapshot matrix (paused panels, disabled graph circuits, switch detail) × themes × widths; gallery regeneration.