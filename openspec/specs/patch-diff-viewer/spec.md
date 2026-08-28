# patch-diff-viewer Specification

## Purpose
TBD - created by archiving change patch-diff-viewer. Update Purpose after archive.

## Requirements

### Requirement: Order-independent structural diff
The system SHALL compare two patches by key rather than position, so that patches with the same circuits in a different file order compare as equal. Component identity SHALL use `HwComponent.id` (e.g. `B1.1`); circuit identity SHALL use `NodeId = (circuit_name, instance_index)`; cable identity SHALL use the cable name.

#### Scenario: Reordered panels are equal
- **WHEN** two patches have identical circuits, wiring, and settings but the HW modules appear in a different file order
- **THEN** the diff reports no differences

#### Scenario: Added and removed circuits
- **WHEN** one patch contains a circuit whose `NodeId` is absent from the other
- **THEN** the diff reports that `NodeId` as added or removed

### Requirement: Wiring comparison
The system SHALL compare the cable topology from `cable_index`: per cable, the sources as a set (order-insensitive) and the sink references as a set of `(sink_circuit, sink_param)` resolved to `NodeId`. Cables present in only one patch SHALL be reported as added/removed; the same cable with different sinks SHALL be reported as changed. LED associations SHALL be compared alongside wiring.

#### Scenario: Cable added
- **WHEN** a cable name exists in patch B but not patch A
- **THEN** the diff reports that cable as added

#### Scenario: Cable sink changed
- **WHEN** a cable exists in both patches but its `sink_refs` set differs
- **THEN** the diff reports that cable as changed

### Requirement: Settings comparison
The system SHALL compare non-wiring parameters per circuit: every `[section]` key/value whose value is not a `_CABLE`, keyed by `NodeId`, compared as a `HashMap<param_key, param_value>` (value-string equality). Parameter order inside a section SHALL be irrelevant. A circuit present in only one patch SHALL be reported as added/removed; a circuit with differing parameter values SHALL be reported with per-key differences.

#### Scenario: Changed parameter value
- **WHEN** a parameter value differs between two otherwise identical sections
- **THEN** the diff reports that parameter as changed for that `NodeId`

#### Scenario: Parameter reorder is equal
- **WHEN** two sections have the same key/values in a different parameter order
- **THEN** the diff reports no difference for that `NodeId`

### Requirement: Scoped diff
The system SHALL support a patch-wide diff (the default) and a component-scoped diff filtered to bindings and parameters that involve a selected hardware token. The scope SHALL be a filter over the computed `DiffReport` using the existing selection state (`App.selected_component`) and its influence.

#### Scenario: Component scope filters report
- **WHEN** a hardware token is selected as the diff scope
- **THEN** the diff shows only cables and parameters that involve that token's influence

### Requirement: Graph highlighting
The system SHALL render diff differences in the signal-flow graph view using two new edge tokens — `graph_edge_diff_added` and `graph_edge_diff_removed` — with precedence `error` > `diff` > modifier > `CableKind`. The system SHALL mark graph node titles where a circuit's parameters differ and SHALL tint cluster containers when all their members are added or removed.

#### Scenario: Difference edges highlighted
- **WHEN** a diff is active and the graph surface is open
- **THEN** added cables render with `graph_edge_diff_added` and removed cables with `graph_edge_diff_removed`

#### Scenario: Error precedence preserved
- **WHEN** a cable is both a topology error and a diff difference
- **THEN** it renders with the topology-error token, not the diff token
