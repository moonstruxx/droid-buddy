# Spec Delta

## Purpose

Keep hardware and circuit labels editable outside the patch file, and resolve a useful label for every circuit that has none.

## ADDED Requirements

### Requirement: Derived circuit labels from the signal-flow tree

When a circuit has no stored label, the system SHALL derive its display label from the signal-flow graph: walk upstream from that circuit (the circuit itself excluded) in deterministic breadth-first order over the graph's reversed edges and return the first explicit label found — a stored label of an upstream circuit, or a stored-or-preamble label of an upstream controller or jack (port) token. Derived names (raw circuit names and derived kind-plus-token names such as `Button B3.17`) SHALL NOT count as explicit labels. The circuit display resolution chain SHALL be stored label → tree-derived label → raw circuit name, and a stored label SHALL always win. The walk SHALL be cycle-safe and deterministic: the same graph and patch SHALL yield the same derived label. Derivation SHALL be available only while the graph is built; without a built graph, circuit labels resolve stored label → raw circuit name.

#### Scenario: Nearest upstream explicit label wins

- **WHEN** `envelope` is driven through an unlabeled `pulser` by controller token `B3.17`, which has stored label `[RATC]`
- **THEN** `envelope` displays `[RATC]` while it has no stored label of its own

#### Scenario: Stored label beats the derived label

- **WHEN** `pulser` has stored label `Clock` and an upstream port has stored label `[RATC]`
- **THEN** `pulser` displays `Clock`

#### Scenario: Derived names never count as guesses

- **WHEN** the only labels upstream of `pulser` are derived names (no stored or preamble label anywhere upstream)
- **THEN** `pulser` displays its raw circuit name

#### Scenario: Deterministic derivation

- **WHEN** the same patch and graph are rendered twice
- **THEN** every derived circuit label is identical across renders

#### Scenario: No graph built, no guess

- **WHEN** the source viewer is open but the graph has never been built
- **THEN** circuit labels resolve stored label → raw circuit name with no tree-derived candidate
