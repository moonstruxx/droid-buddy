## Why

The graph is circuits plus virtual cables only. Real DROID signal flow also runs through the hardware: a button circuit reads `B1.1`, an LED assignment writes `L1.1`, a CV in reads `I1`, an output jack is driven by `O3`. DroidGraph draws these as nodes and edges, and the data to classify them is already in this codebase: the embedded schema's `JACK_TABLE` (schema.rs) lists every register family, and `Patch` already recognizes controller-declaring sections and numbers them in chain order. This change draws those registers so the graph matches what the patch actually touches, which is the foundation the select-state filtering (change C) and the dependency walker (change D) build on.

## What Changes

- The graph node model gains three node kinds next to circuits: controller, input jack, and output jack. Controller nodes come from the controller-declaring sections the parser already recognizes (`[p2b8]`, `[b32]`, ...), numbered by chain order; jack nodes are created on demand for master registers (`I1`, `O3`, `G1.4`, ...).
- Register references in circuit assignments become directed edges, with direction taken from the same catalog classification change A introduced. A circuit writing a register produces an edge to the controller node (for controller registers like buttons, LEDs, pots, switches, encoders) or to an output-jack node (for master CV/gate outputs); a circuit reading a register consumes an edge from the controller node or from an input-jack node.
- A register belongs to a declared controller when its letter is a controller register letter and its unit number matches a declared controller ordinal; everything else resolves to a jack node on the master.
- Identical edges are deduplicated, matching the existing cable behavior.

## Capabilities

### New Capabilities

- signal-flow-graph: a new requirement "Register and hardware nodes" covering controller, input-jack, and output-jack nodes and their directed register edges.

### Modified Capabilities

- signal-flow-graph: the "ComfyUI-style rendering" requirement gains per-kind node frames and a register-edge color token.

## Impact

- src/graph.rs: `NodeKind` enum, `NodeId` widened to an enum (Circuit/Controller/Jack), `GraphNode.kind`, register-edge building in `build_edges`, controller-node collection. All `NodeId` consumers (`app.rs` pinned set, `handler.rs`, `ui.rs`, `graph_render.rs`, `layout.rs` call sites) adapt to the new type.
- src/patch.rs: a boundary-aware `scan_register_refs` helper over the `JACK_TABLE` prefix set, separate from the existing component scanner; an ordered controller list (type + ordinal) exposed for graph building.
- src/theme.rs + src/ui.rs + src/graph_render.rs: per-kind node colors and a `graph_edge_register` token in all three built-in palettes.
- src/layout.rs: no API change; controller and jack nodes simply join the deterministic solve as ordinary nodes.
- src/diff.rs, src/validation.rs, topology checks: unchanged. Register edges are not cables and never feed the cable topology-error machinery or the diff model.

## Non-goals

- No dangling-cable nodes (separate change F).
- No select-state filtering or hiding of unselected circuits (change C).
- No upstream dependency-walker mode (change D).
- No change to the influence traversal: it stays cable-only.
- No change to `scan_hw_tokens` or the hardware-component pipeline.