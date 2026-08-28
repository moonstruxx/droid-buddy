# Proposal: patch-diff-viewer

## Why

A DROID patch evolves across edits, and `droid_tui` gives the user no way to see *what changed* between two versions. Because a patch is a `.ini` with repeated section names and a hardware-cable topology, a naive line diff is useless: reordering HW modules (moving `[button]` blocks around) produces a huge textual diff for a patch that is functionally identical. The meaningful comparison is **structural** — the wiring (cable graph) and the per-circuit settings — independent of HW module order.

`droid_tui` already parses patches into a rich `Patch` model (`hw_components`, `sections` keyed by instance, `cable_index`) and already renders a signal-flow graph with per-edge color tokens. It is therefore well positioned to compare two patches structurally and highlight the differences right in the graph view the user already has.

## What Changes

- **Order-independent structural diff** — compare two parsed `Patch` objects by *key, not position*: `hw_components` keyed by `HwComponent.id`, sections by `NodeId = (circuit, instance_index)`, `cable_index` already keyed by cable name. Reordered-but-identical patches compare as equal.
- **Wiring comparison** — per-cable equality over `CableIndexEntry.sources` (set) and `sink_refs` (set of `(sink_circuit, sink_param)` resolved to `NodeId`). Cables in one patch only = added/removed; same cable with different sinks = changed. LED associations compared alongside.
- **Settings comparison** — per-`NodeId` `HashMap<param_key, param_value>` (value-string equality); param order inside a section irrelevant. Added/removed circuits = whole `NodeId`; changed = per-key diff.
- **Scoped mode** — diff is patch-wide by default, or filtered to bindings/params involving a selected HW component token (reusing `App.selected_component`). Scoping is a filter on the already-computed `DiffReport`, not a separate pass.
- **Visual highlighting in the graph** — two new `Theme` edge tokens (`graph_edge_diff_added`/`graph_edge_diff_removed`) plus node-title markers for circuits whose params differ and a cluster tint. Precedence: `error` > `diff` > modifier > `CableKind`.
- **Key flow** — `g d` opens the picker for the B patch; `d` toggles the diff overlay; `Esc` clears scope then overlay. New pure `src/diff.rs` module + `Event::DiffComputed`.

**Non-goals** (YAGNI):
- No three-way merge or apply — this is a read-only *viewer* of differences.
- No write-back/metadata persistence of the diff state across sessions.
- No diff of the DROID circuit *schema* manifest — only of parsed patch content.

## Capabilities

### New Capabilities
- `patch-diff-viewer`: order-independent structural comparison of two DROID patches — wiring and settings — patch-wide or scoped to a selected HW component, highlighted in the signal-flow graph view.

### Modified Capabilities
<!-- None. The signal-flow-graph surface (cable_color precedence, node/cluster rendering) is extended, not changed in behavior. -->

## Impact

- **Code:** new `src/diff.rs` (pure `diff_patches` -> `DiffReport`), `src/app.rs` (`diff_patch`/`diff_report`/`diff_scope`, `load_diff_patch`, `clear_diff`), `src/events.rs` (`Event::DiffComputed`), `src/handler.rs` (`g d` picker flow, `d` toggle), `src/ui.rs` (diff-aware `cable_color`, node markers, cluster tint, source `+`/`-` markers), `src/theme.rs` (two new edge tokens + palettes).
- **Data:** none new (diff state is in-memory only).
- **Dependencies:** none new.
- **Tests:** `src/diff.rs` unit tests (order independence, wiring added/removed/changed, settings per-key), app/handler wiring, graph-highlighting snapshot matrix (`classic`/`terminal`/`mono` × widths, mirroring `visual-validation`).
