# Visual Validation Specification

## Purpose

Generate deterministic visual evidence of UI rendering for review and CI. The physical scale model adds skeleton | full side-by-side proof rows to the gallery matrix.

## ADDED Requirements

### Requirement: Skeleton and full proof rows

The system SHALL render each physical-layout gallery scenario twice — once in skeleton mode and once in full mode — side by side in the matrix, proving the full render coincides with the grid model.

#### Scenario: Gallery shows both presentations

- **WHEN** the gallery matrix is generated
- **THEN** each physical-layout scenario row includes a skeleton-mode frame and a full-mode frame at the same viewport.

#### Scenario: Coincidence is asserted

- **WHEN** both frames are produced for a scenario
- **THEN** the test harness asserts every full-mode element rect equals the corresponding skeleton cell.