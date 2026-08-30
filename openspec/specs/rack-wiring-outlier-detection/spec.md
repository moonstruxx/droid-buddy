# rack-wiring-outlier-detection Specification

## Purpose

Detect physically implausible HwComponent bindings in a DROID patch by comparing element-precise rack distance against cable hops, so wiring errors that the circuit-list editor hides are surfaced as topology warnings in the signal-flow graph view.

## Requirements

### Requirement: Rack geometry table
The system SHALL provide a machine-readable rack geometry that maps every HwComponent token to an element-precise position in B32-grid units. The table SHALL describe controller positions and per-controller element offsets, support vertical mounting, and cover multiple racks (distinct coordinate bands).

- The B32 controller SHALL be modeled as a 4×8 grid (4 columns × 8 rows) numbered row-wise: row 0 is `B1.1..B1.4`, row 1 is `B1.5..B1.8`, row 2 is `B1.9..B1.12`, row 3 is `B1.13..B1.16`, row 4 is `B1.17..B1.20`, row 5 is `B1.21..B1.24`, row 6 is `B1.25..B1.28`, row 7 is `B1.29..B1.32`. LEDs (`L1.1..L1.32`) are co-located with their buttons (distance 0).
- Uppercase/lowercase controller names that denote the same physical grid (e.g. `B32` and `b32`, `E4` and `e4`) SHALL share the same element grid.
- Each binding to a co-located `L→B` pair SHALL be treated as distance 0 and never flagged as an outlier.

#### Scenario: B32 button position
- **WHEN** a patch references `B1.17`
- **THEN** the geometry resolves it to row 2, column 0 of its B32 slot

#### Scenario: Co-located LED and button
- **WHEN** a patch binds `L1.17` to `B1.17`
- **THEN** the binding distance is 0 and it is never flagged

#### Scenario: Same grid for mirrored controller names
- **WHEN** a patch references `b32` and `B32`
- **THEN** both resolve through the same element grid

### Requirement: Element-precise binding geometry
The system SHALL compute, for every HwComponent binding, a source position, a sink position, the Euclidean and Manhattan distance between them in grid units, whether they are adjacent, whether they share a controller/rack, and the number of cable hops between source and sink.

#### Scenario: Far direct wire
- **WHEN** a binding connects `E4.4` (left modifier encoder) to `M4.2` (fader) with zero cable hops
- **THEN** the geometry reports a large distance and `cable_hops == 0`

#### Scenario: Adjacent binding
- **WHEN** a binding connects `B1.17` to `B1.18`
- **THEN** the geometry reports an adjacent (distance 1) pair

#### Scenario: Via-cable binding
- **WHEN** a binding reaches a distant target through one or more `_cable` hops
- **THEN** the geometry reports `cable_hops > 0` for the same physical distance

### Requirement: Wiring-outlier topology warning

The system SHALL flag a binding as a wiring outlier when a compact learned decision artifact, fitted offline from a rebalanced labeled corpus and embedded in the binary, classifies the binding as implausible — and it is wired directly (zero cable hops) — producing a topology warning that the signal-flow graph renders with the error-highlight token. Bindings that reach a distant target via a cable, or that are physically adjacent or co-located, SHALL NOT be flagged regardless of the artifact's output. The artifact SHALL be a static, deterministic, in-process scorer with no external model runtime; a binding matching no artifact row SHALL fall back to the preserved threshold rule (physical distance exceeding the configured threshold, zero cable hops), never to silence.

#### Scenario: Outlier flagged
- **WHEN** a binding connects a far-left modifier encoder directly to a far-right fader without a cable
- **THEN** the graph renders the offending edge with the error-highlight token and reports a topology warning

#### Scenario: Via-cable not flagged
- **WHEN** a binding reaches a far target through a cable
- **THEN** the binding is not flagged, even if the artifact would otherwise classify it as implausible

#### Scenario: Adjacent not flagged
- **WHEN** a binding connects adjacent buttons
- **THEN** the binding is not flagged, even if the artifact would otherwise classify it as implausible

#### Scenario: Co-located LED and button never flagged
- **WHEN** a binding connects a co-located `L→B` pair
- **THEN** the binding is not flagged, even if the artifact would otherwise classify it as implausible

#### Scenario: Artifact replaced without behavior change
- **WHEN** the embedded artifact is refitted from a new corpus snapshot
- **THEN** no change to the warning channel, the render path, or the topology validation flow is required

### Requirement: Per-token influence second opinion

The system SHALL surface a topology warning for a hardware token whose `influence_subtree` size deviates from the corpus distribution for its token kind by more than a calibrated threshold (a z-score), using corpus mean and standard deviation baked into the binary. The second opinion SHALL travel through the same `TopologyIssue` warning channel and error-highlight token as the wiring-outlier finding, and SHALL NOT gate patch loading.

#### Scenario: Statistically unusual influence flagged
- **WHEN** a patch contains a hardware token whose influence subtree is an extreme outlier for its token kind
- **THEN** the graph renders the associated cable with the error-highlight token and reports a topology warning

#### Scenario: Typical influence not flagged
- **WHEN** every hardware token's influence subtree size is within the calibrated z-score band
- **THEN** no additional topology warning is produced

#### Scenario: Warning does not block loading
- **WHEN** a per-token z-score warning is present
- **THEN** the patch still loads and the warning appears only in the topology findings
