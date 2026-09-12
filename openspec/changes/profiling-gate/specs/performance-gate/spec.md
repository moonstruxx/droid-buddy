# performance-gate Specification

## Purpose

Catch CPU hogs and endless loops in the core pure phases by running them over the real workload anchors under calibrated wall-clock budgets, and keep a deep-dive profiling workflow so a busted budget produces evidence.

## ADDED Requirements

### Requirement: Budgeted workload phases

The system SHALL run each gated phase over the scale-anchor fixture (`droid_mpfs5melody2.ini`, 532 sections / 457 cables / 115 hw tokens) under a named wall-clock budget.

#### Scenario: Parse-validate-render completes within budget

- **WHEN** the gate parses, validates, and renders the scale anchor
- **THEN** the phase completes within its budget and the elapsed time is reported

#### Scenario: Graph build and solve complete within budget

- **WHEN** the gate builds the signal-flow graph from the scale anchor and runs a full force-directed solve
- **THEN** the phase completes within its budget and the elapsed time is reported

#### Scenario: Optimizer search completes within budget

- **WHEN** the gate generates latency-optimization candidates for the scale anchor
- **THEN** the phase completes within its budget and the elapsed time is reported

### Requirement: Bounded render throughput

The system SHALL render the full gallery matrix (every scenario and theme) within a named render budget.

#### Scenario: Gallery matrix completes within budget

- **WHEN** the gate renders every gallery scenario under every theme
- **THEN** the matrix completes within its budget and the elapsed time is reported

### Requirement: Watchdog-protected phases

Each gated phase SHALL run under a watchdog that fails the gate instead of hanging the suite when the phase exceeds its ceiling.

#### Scenario: A looping phase busts the gate

- **WHEN** a phase exceeds its wall-clock ceiling (an unbounded loop or a pathological slowdown)
- **THEN** the gate fails with the phase name and the elapsed time, and the suite continues to a red result instead of hanging

### Requirement: Calibrated budgets

Budget constants SHALL be generous multiples of the measured release baselines so the gate never flakes on slow machines while still tripping on genuine regressions.

#### Scenario: Budgets carry baseline documentation

- **WHEN** the gate ships
- **THEN** each budget constant documents the measured baseline it derives from

### Requirement: Deep-dive profiling workflow

The system SHALL ship a script that records a chosen phase under `perf` and renders a flamegraph for post-mortem analysis.

#### Scenario: Profile evidence produced

- **WHEN** a budget busts and the developer runs the profiling script for the phase
- **THEN** a flamegraph SVG and the raw `perf.data` are written under `.opencode/.tmp/profiling/`

### Requirement: Gate runs in release mode

The perf gate SHALL run in release mode so budgets measure honest release timings.

#### Scenario: CI executes the gate in release

- **WHEN** CI runs the verification gate
- **THEN** the perf gate executes against the release build and a busted budget fails the run