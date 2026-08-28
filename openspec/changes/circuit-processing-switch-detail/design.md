## Context

The app simulates component state locally from parsed `.ini` files; a patch is a set of circuit instances (repeated section names) connected by virtual cables. Today every render happens live and every circuit always "runs". The graph surface (`g g`) already publishes `graph_node_rects` and tracks hover; the modifier influence walk is a structural forward-BFS over `cable_index`. Switch components currently render with the button token and only show `▣ ON` / `□ OFF`.

## Goals / Non-Goals

**Goals:**
- A global processing pause that freezes simulated state mutation while keeping navigation/selection working.
- Per-circuit processing disable (keyed by circuit name + instance index) exposed through the graph surface.
- Dimmed rendering for paused panels and disabled graph nodes/edges.
- A dedicated `switch` theme token and value-percentage rendering for positional switches.

**Non-Goals:**
- A real runtime/scheduler — the app stays a pure viewer; "processing" is a display flag plus blocked mutation.
- Additive modifier union — single-var influence remains.
- Picker-visible controls or persistence of per-circuit state across patch loads.

## Decisions

1. **Pause is a display-mode flag, not a state freeze.** `App.processing_paused: bool` blocks state-mutating handlers (Enter/Space toggles, mouse toggles, knob/fader scroll) in `handler.rs` while leaving selection/navigation/picker/prefix/graph drags active. No simulation clock exists to stop, so nothing else changes.
2. **Per-circuit disable keyed by `(name, instance_index)`.** `App.disabled_circuits: HashSet<(String, usize)>`, consulted by: the influence walk in `app.rs`/`patch.rs` (a disabled circuit's outputs do not propagate — its own cells stay marked influenced, downstream do not), the graph renderer (`cable_color`/node styling → dim token), and graph rebuild.
3. **`x` in the graph surface** toggles the hovered node's circuit (via `graph_node_rects` hit-testing, same pattern as drag); emits `GraphRebuilt` and recomputes influence on toggle. `p` is a global key outside the picker, mirroring `q`/`l` placement.
4. **Dim rendering reuses existing dimming patterns** (shift-dimming / modifier dimming already in `ui.rs`) — panels dim via the same style helper while paused; graph nodes/edges dim with a `dim` modifier over their normal token.
5. **`switch` token** added to `Theme` with `classic` = white (byte-identical snapshots), `terminal` = Reset, `mono` = dark-gray (distinct from button gray). Switch `Value` state renders `◉ {:.0}%` mirroring knob/encoder.
6. **Status messaging** on every pause/resume and circuit toggle, reusing `status_message`.

## Risks / Trade-offs

- `x` reuses the `x` key in graph surface — check no existing graph-surface key conflicts (graph surface currently owns only drag + Esc/q/l; safe).
- Disabled-circuit influence semantics are a deliberate model choice (influenced but non-propagating) — mirrors the physical act of a circuit not processing its outputs; documented in the spec.
- Snapshot surface: pause/circuit-disable states are only reachable via new keys, so existing snapshots stay stable; only the switch token change touches existing rendering and is designed to be byte-identical in `classic`.