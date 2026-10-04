# graph-arrange Specification

## Purpose

Manual control over signal-flow graph arrangement: one key that always produces a visible, fitted layout and cycles the available arrangement algorithms on repeat presses, plus a guarantee that optimizer-driven rebuilds never leave a stale camera behind. This capability exists because the automatic preview refit can silently skip and pin anchors can scramble across section reorders.

## Requirements

### Requirement: Arrange key applies current arrangement

The graph pane SHALL provide an `a` key that rebuilds the graph positions under the active arrangement and refits the camera to the live graph-pane rect, reporting the active arrangement in the status line.

#### Scenario: Pressing arrange visibly arranges

- **WHEN** the graph pane holds focus and the user presses `a`
- **THEN** the node positions are re-solved under the active arrangement and the camera frames the re-solved layout in the live pane.

#### Scenario: Arrange is a silent no-op without a graph

- **WHEN** no graph is built and the user presses `a` on the graph pane
- **THEN** nothing changes and no error is reported.

### Requirement: Repeat presses cycle arrangements

Repeated presses of `a` SHALL cycle the arrangement order `Column-Strict → Column-Barycenter → Force → Column-Strict …`, persisting the selection like the existing layout mode/ordering and rebuilding + refitting on every step.

#### Scenario: Cycling order

- **WHEN** the user presses `a` three times from `Column-Strict`
- **THEN** the arrangements applied are `Column-Strict`, `Column-Barycenter`, `Force`, and the fourth press returns to `Column-Strict`.

### Requirement: Optimizer preview always refits

After every optimizer preview rebuild the camera SHALL frame the re-solved layout against the live graph-pane rect, never silently skipping when the last published canvas size is stale or `None`.

#### Scenario: Preview after pane reshuffle stays visible

- **WHEN** the optimizer opens (reshuffling panes) and the user previews a candidate
- **THEN** the graph re-solves and the camera fits the live pane, so the reorder is visible without further input.
