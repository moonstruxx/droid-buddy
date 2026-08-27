## Purpose

Forward influence tracing for DROID virtual-cable modifiers: given a modifier-produced variable (`_VAR` written by circuits referencing a selected hardware token), compute the influenced subgraph through indirect hops (e.g. `switch`) — for highlighting the full graph and deriving the isolated filtered graph.

## ADDED Requirements

### Requirement: Influence subtree walk

The system SHALL compute the influenced region for a modifier variable via a forward BFS walk over the cable graph, with structural hop continuation: `cable -> sink circuits (any param consuming _VAR) -> if sink has output ports (output = _X) and is on the current signal flow, queue its outputs and continue`. Hop eligibility is structural (any circuit with input+output ports on the current flow), not an allowlist by circuit name.

- Cycle-safe (visited cables/circuits sets), deterministic (sorted iteration), pure (no terminal/IO).
- Input is one or more root variables (`_VAR` set derived from the selected hardware token's producing circuits: circuits whose params reference the token and have `output = _VAR`).
- Output is `{ influenced_nodes: Set<NodeId>, influenced_edges: Set<cable> }` where `NodeId = (circuit_name, instance_index)`.

#### Scenario: Direct consumption

- **WHEN** `_CLOCK` is produced by `[clock]` and consumed by `[arpeggio] select = _CLOCK`
- **THEN** `influenced_nodes` includes the `arpeggio` instance node and `influenced_edges` includes `_CLOCK`; the full-graph highlight bolds that edge/node, and the filtered graph contains that single-edge subgraph.

#### Scenario: Indirect hop via switch

- **WHEN** `_SEL` is consumed by a `switch` circuit (`select = _SEL, input = _A, output = _B`) and `_B` is consumed downstream by `[quantizer] input = _B`
- **THEN** the walk reaches `switch` as a sink of `_SEL`, sees `switch` has an output port (`_B`), queues `_B`, and also marks `quantizer` influenced; both edges `_SEL` and `_B` and both nodes `switch` and `quantizer` appear in the influence sets. The filtered graph contains `switch -> quantizer` chained.

#### Scenario: Cycle-safe termination

- **WHEN** cables form a cycle (`_A` sinks to circuit producing `_B`, `_B` sinks back to circuit producing `_A`)
- **THEN** the walk terminates without infinite loop, visiting each cable/circuit at most once, and the result remains finite and deterministic.

#### Scenario: Leaf termination

- **WHEN** a sink circuit has no `output = _X` (no output port) — e.g. a pure sink like `[led]` or a terminal effect
- **THEN** the walk marks it influenced but does not queue further cables; it is a leaf.

#### Scenario: Any input+output circuit qualifies as hop

- **WHEN** an arbitrary circuit (not just `switch`) has both an input-consuming param and an `output = _VAR` param and lies on the current flow (is a sink of a path cable)
- **THEN** it is treated as a hop and its outputs continue the walk; a whitelist of circuit names MUST NOT be used to decide hop eligibility.

### Requirement: Variable derivation from hardware selection

The system SHALL derive root variable(s) from a selected hardware token (e.g. `B1.1`) as: all `_VAR` where some circuit section references the token in a param value and has `output = _VAR` in the same section. If multiple variables are produced (e.g. one button drives two outputs), the union of their influence walks is the result.

#### Scenario: HW -> variable derivation

- **WHEN** hardware token `B1.1` is selected and a `[button]` section contains `button = B1.1` and `output = _TRIG`
- **THEN** the root set is `{ _TRIG }` and the walk starts from `_TRIG`.

### Requirement: Filtered induced subgraph

The system SHALL derive a filtered graph as the induced subgraph on `influenced_nodes` with edges from `influenced_edges` whose endpoints are both in the influenced set, plus their cluster membership inherited from the full graph's banner clusters.

#### Scenario: Filtered graph membership

- **WHEN** the influence set contains nodes `{copy, switch, quantizer}` and edges `{_CLOCK, _B}`
- **THEN** the filtered graph's nodes/edges/clusters are exactly that subset (no uninfluenced nodes leak in), and it is independently solvable with its own compact layout.

