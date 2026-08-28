## Why

Users annotate DROID patches in the preamble (`#  B3.17: [RATC] ...`), but the TUI has no editable label layer: HW cells show derived names (`Button B3.17`) and circuit headers show raw circuit names (`[motorfader]`). For multi-shift patches (your MPFS 379-section melody, `I4:` empty quirk, `select = _A * _B` chains) users need per-shift HW labels and per-instance circuit labels without mutating the `.ini`. Preamble is read-only human commentary; overlays let users relabel in place while the graph/source influence highlight makes modifier wiring visible during editing.

## What Changes

- Per-patch XDG label store `~/.config/droid-tui/labels.toml` (mirrors `config.rs` atomic tmp→rename, warn-once): two namespaces — `hw` per `HwToken` per `ShiftGroup` slot and `circuits` per `NodeId=(circuit,instance)`. Keyed by canonicalized absolute patch path string (content-hash follow-up noted).
- Config `[labels]` with `layers_enabled = true` (master switch) and `max_shift_layer = 4` (1..8 clamped, default 4). When disabled or clamped, `display_label` coerces to layer 1; store data for 2..N is preserved.
- HW labels: uniform 4 slots for every token (`B*`/`P*`/`S*`/`E*`/`I*`/`G*`/`O*`). Merge chain `store[layer] → store[1] → preamble[1] → derived`.
- Circuit labels: single label per instance, renders as `source` section-header override and `graph` node-title override in both FULL and FILTERED panes.
- Inline edit overlay (`e` on focused datum): panels → `B3.17 / Group N`, source → section header, graph → hovered node (`hovered_graph_node`). Single-field + `1`–`4` (range follows `max_shift_layer`) cycle inside editor; status shows `B3.17 / Group2 → 3 ckts / 2 cables` via structural `influence_subtree` and `modifier_hue(hash%16)`. `Esc` cancels, `Enter` saves.
- Highlight stays structural (BFS `influence_subtree` over `cable_index`+`circuit_outputs`, shift-blind, second B replaces first — additive aspirational deferred) with same hue across panels/source/graph (`graph_edge_error` red > modifier hue > `CableKind`). No `.ini` mutation, no network, no async.

## Capabilities

### New Capabilities
- `label-management`: per-patch HW per-shift + per-circuit label store, overlays, and display overrides with influence-aware edit status.
- `label-configuration`: `[labels]` config for `layers_enabled` and `max_shift_layer` with clamping and disabled coercion.

### Modified Capabilities
- `controller-panels`: display label for HW cells becomes `Patch::display_label` overlay fallback rather than preamble/derived only; geometry unchanged.
- `source-navigation`: section header text becomes circuit-label override when present; scroll/occurrence behavior unchanged.
- `signal-flow-graph`: node title becomes circuit-label override when present; layout/physics unchanged.
- `config`: add `[labels]` table with validation and persistence alongside `theme`.

## Impact

- `src/patch.rs` — `Patch::display_label(token, shift)`, preamble fallback helper, circuit-label accessor; pure, no I/O.
- `src/config.rs` — `Settings.labels { layers_enabled: bool, max_shift_layer: u8 }`, XDG load/save, clamping, warn-once.
- `src/app.rs` — `LabelStore` (load per patch, mutation, atomic save), wiring to `display_label` consumers + `recompute_influence` for edit status.
- `src/handler.rs` — `e` edit entry/exit, `1`–`4` in-editor layer switch, `Enter`/`Esc`, focus routing for source/graph/panels.
- `src/ui.rs` — overlay z-layer over quad, label overrides for panel cells / source headers / graph nodes, status hue.
- `src/theme.rs` — no new token (reuses `modifier_hue`), keeps semantic-token discipline.
- Tests: unit for merge/clamp/disabled, regression `TestBackend` for overlay per theme/width, `I4:` empty-slot fixture, gallery delta.

## Non-goals

- Mutating `.ini`, Acified-style N-bag menus beyond 4 layers, shift-aware `selectat`/switch-position filtering, MIDI/hardware bridge, persistence of influence latch across loads, network/ML outlier detection (separate roadmap `droid_tui-nnq` P4).
