## Context

The graph model (graph.rs) is circuits-only: `NodeId = (String, usize)`, `GraphNode { id, circuit, instance_index, section_index }`, and `build_edges` turns the parsed cable index into directed cable edges. Register references (buttons, LEDs, pots, CV jacks) exist in the parsed sections but never appear in the graph. The parser already knows the controllers: controller-declaring sections (`[p2b8]`, `[b32]`, ...) are recognized via `BARE_SYNTHESIS`/`KNOWN_CONTROLLER_SECTIONS` and numbered in chain order (`controller_chain_pos`, `controller_types: HashMap<u32, String>`), and `schema.rs` carries the full `JACK_TABLE` of register families (I, O, G, B, L, P, M, S, E, N, R). Change A makes the catalog (`get_param_kind`) authoritative for cable direction; this change applies the same classification to register references.

DroidGraph draws the full signal graph: controller units, input/output jacks, and circuits as nodes, and every register read/write as an edge, direction decided by the circuit catalog. Its classification rule: a register belongs to a declared controller when its letter is in `PBESLR` and a controller with that unit number exists; otherwise it is a jack on the master (input when the letter is I, N, or a controller-register letter without a matching unit, output otherwise). Identical edges are deduplicated.

## Goals / Non-Goals

**Goals:**

- Draw controller, input-jack, and output-jack nodes and directed register edges, with direction from the same catalog classification change A introduces.
- Reuse the parser's existing controller recognition so the graph's controllers agree with the panel view.
- Keep cable edges, topology validation, diff, and the influence traversal exactly as they are.

**Non-Goals:**

- Any change to `scan_hw_tokens` or hardware-component extraction (the new register scanner is a separate helper for the graph).
- Register edges participating in topology validation or the diff model.
- Select-state filtering (change C), dependency walker (change D), dangling-cable nodes (change F).

## Decisions

- **Widen `NodeId` to an enum.** `NodeId::Circuit(String, usize)` keeps the current identity, `NodeId::Controller(String, usize)` is (type, ordinal), `NodeId::Jack(String)` is the token (`I1`, `O3`, `G1.4`). An alternative is encoding kinds into the existing `(String, usize)` pair with synthetic names, but that leaks the encoding into every consumer and makes the diff/influence code that keys on node identity error-prone. The enum is a mechanical change: all consumers are in-repo, `layout::solve` already works on node indices rather than ids, and `App.pinned: HashSet<NodeId>` and `Graph.highlighted_nodes: HashSet<NodeId>` compile as-is once the constructor calls adapt.

- **Controller nodes come from the parser's controller recognition.** `Patch` already identifies controller-declaring sections and numbers them in chain order. The graph build asks `Patch` for the ordered `(type, ordinal)` list and creates one node per controller, labeled with the panel name (`P2B8 #1`, `CV I/O #1`). Using the same source keeps the graph's controller set identical to the physical panel view.

- **A dedicated `scan_register_refs` helper, not `scan_hw_tokens`.** The component scanner covers B/L/P/O/I/E/S and feeds `add_component`; changing its letter set would alter hardware-component extraction, a wider blast radius than this change needs. The graph-only helper scans values for the `JACK_TABLE` prefix set with the same boundary rule (a letter followed by digits, optional `.channel`, non-alphanumeric boundary before), returning `(token, unit, pin)`. It lives next to `scan_hw_tokens` in patch.rs.

- **Register-edge direction reuses `get_param_kind`.** A parameter the catalog marks as an output writes its register references (circuit to target); an input reads them (source to circuit). For a circuit or parameter absent from the catalog, the fallback is the DroidGraph convention: a parameter family of `output` or `led` writes, everything else reads. This mirrors change A's fallback so cable and register direction never disagree for the same assignment.

- **On-controller classification follows DroidGraph.** A register token is on a declared controller when its letter is a controller register letter and its unit is a declared controller ordinal. The controller-register letter set is `PBESLR` (matching DroidGraph's `ControllerRegisterLetters`, derived from the firmware): P pots, B buttons, E encoders, S switches, L LEDs, R master controller registers. I, O, G, M, N are master jacks: a circuit writing them produces an output-jack node, a circuit reading them consumes an input-jack node. A controller-register letter without a matching unit (for example `P3.1` when only two controllers are declared) falls back to a jack node as well, input when the assignment reads it, output when it writes it.

- **Jack nodes are created on demand and shared.** `GetOrAddJack` semantics from DroidGraph: the first assignment touching a token creates the jack node, later assignments reuse it. A jack's direction is fixed at creation (input vs output) because a physical jack is one or the other; the classification above decides it deterministically, so two assignments disagreeing on a token resolve to the same node.

- **Register edges join the deterministic solve as ordinary nodes.** Controller and jack nodes participate in the layered seed and refinement like circuits. A heavily-used controller becomes a hub; the seed's topological-depth placement puts sources (controllers, input jacks) on the left and sinks (output jacks) on the right, which is where they belong. If hub density proves visually noisy on real patches, the follow-up is pinning controller nodes at seed positions, not changing the solver.

## Risks / Trade-offs

- [A register letter belongs to a controller in the manual but is not in the set] → the set is a single constant, tested against fixtures; extending it is a one-line change. The classification test matrix pins the current set.

- [Controller hubs dominate the layout] → accepted; the layered seed already biases sources left and sinks right. A future pinning of controller nodes is noted in Decisions, not scoped here.

- [Jack direction disagreement between assignments] → a physical jack is fixed; the deterministic classification resolves it. The test matrix covers the input/output disagreement case.

- [The `NodeId` enum refactor touches every consumer] → mechanical, in-repo, and `layout::solve` is index-based so the solver is unaffected. The compile surface is `app.rs`, `handler.rs`, `ui.rs`, `graph_render.rs`, and the graph tests.

- [Register edges make the graph busier by default] → that is the point of the change; the existing FULL/FILTERED modes and the later dependency-walker (D) are the taming tools.