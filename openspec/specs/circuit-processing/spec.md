# circuit-processing Specification

## Purpose
Let the user freeze the simulated circuit run (global pause) and isolate individual circuits (per-circuit enable/disable) so signal flow and influence can be inspected without the simulation mutating state.

## Requirements

### Requirement: Global processing pause

The system SHALL support a global processing pause toggle (`p` key) that freezes the simulation: while paused, all component state mutations (Enter/Space toggles, mouse toggles, knob/fader scroll adjustments) SHALL be blocked, selection and navigation SHALL still work, the status bar SHALL show `PROCESSING PAUSED`, and selection-driven influence SHALL be cleared. Pausing and resuming SHALL each produce a status message.

#### Scenario: Pause blocks state mutation

- **WHEN** processing is enabled and the user presses `p`
- **THEN** processing becomes paused, the status bar shows `Processing paused (p to resume)`, and pressing Enter/Space on a component does not change its state.

#### Scenario: Resume restores mutation

- **WHEN** processing is paused and the user presses `p` again
- **THEN** processing resumes, the status bar shows `Processing enabled (p to pause)`, and component toggles work again.

#### Scenario: Selection survives pause

- **WHEN** processing is paused and the user selects a component
- **THEN** the component is selected but influence is not computed while paused.

### Requirement: Per-circuit processing toggle

The system SHALL support toggling processing for a single circuit instance via the `x` key in the graph surface (and the quad-view GraphFull pane), acting on the circuit of the hovered graph node. The disabled state SHALL be keyed by `(circuit name, instance index)`, SHALL persist for the patch lifetime, SHALL render the circuit's graph node and its edges dimmed (overriding influence highlight), and SHALL make the circuit a dead end in the influence walk (its sinks stay marked influenced, but its outputs do not propagate). Toggling SHALL rebuild the graph and recompute influence, and SHALL produce a status message naming the circuit.

#### Scenario: Toggle hovered circuit in graph

- **WHEN** the graph surface is open, a node is hovered, and the user presses `x`
- **THEN** that circuit instance's processing is disabled, its node and connected edges render dim, and the status bar shows `Processing disabled: <name> <instance>`.

#### Scenario: Toggle back re-enables

- **WHEN** a disabled circuit's node is hovered and the user presses `x` again
- **THEN** the circuit re-enables and its node and edges render normally.

#### Scenario: Influence stops at disabled circuit

- **WHEN** a modifier's influence path passes through a disabled circuit
- **THEN** the disabled circuit itself stays marked influenced but none of its downstream circuits or cables are influenced.

#### Scenario: No hovered node is a no-op

- **WHEN** the graph surface is open, no node is hovered, and the user presses `x`
- **THEN** nothing changes and no status message is emitted.
