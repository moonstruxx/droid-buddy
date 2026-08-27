## Purpose

Delta to `patch-parsing`: add the `circuit -> output cables` reverse map and `influence_subtree` walk as first-class parser outputs (pure, deterministic).

## ADDED Requirements

### Requirement: Circuit-output reverse map

The system SHALL expose, alongside `cable_index`, a map `circuit_outputs: HashMap<NodeId or section_index -> Vec<cable_name>>` listing every `_VAR` produced by each circuit instance via `output = _VAR` (comment-aware, repeated sections preserve distinct instances).

#### Scenario: Reverse map

- **WHEN** a patch has `[copy]` with `output = _A` and a repeated `[copy]` with `output = _B`
- **THEN** the map lists `_A` for the first `copy` instance and `_B` for the second — not merged by name.

### Requirement: Influence walk is pure and deterministic

`influence_subtree` SHALL be pure over strings/structs, cycle-safe, and deterministic (sorted iteration over `cable_index` entries and sink refs), so the same patch + root variable converges to identical highlight sets on the same machine (D9).

#### Scenario: Determinism

- **WHEN** `influence_subtree("_CLOCK")` is called twice on the same patch
- **THEN** the resulting `influenced_nodes`/`influenced_edges` sets are identical (order-stable iteration, no RNG).

