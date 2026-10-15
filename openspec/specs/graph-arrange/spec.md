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

#### Scenario: Arrange applies sections mode

- **WHEN** sections mode is active and the user presses `a`
- **THEN** the graph re-solves under the sections-mode joint black-hole solver and the camera frames the re-solved layout in the live pane.

### Requirement: Repeat presses cycle arrangements

Repeated presses of `a` SHALL cycle the arrangement order `Column-Strict → Column-Barycenter → Force → Sections → Column-Strict …`, persisting the selection like the existing layout mode/ordering and rebuilding + refitting on every step.

#### Scenario: Cycling order

- **WHEN** the user presses `a` three times from `Column-Strict`
- **THEN** the arrangements applied are `Column-Strict`, `Column-Barycenter`, `Force`, and the fourth press returns to `Column-Strict`.

#### Scenario: Cycling order includes sections

- **WHEN** the user presses `a` four times from `Column-Strict`
- **THEN** the arrangements applied are `Column-Strict`, `Column-Barycenter`, `Force`, `Sections`, and the fifth press returns to `Column-Strict`.

### Requirement: Optimizer preview always refits

After every optimizer preview rebuild the camera SHALL frame the re-solved layout against the live graph-pane rect, never silently skipping when the last published canvas size is stale or `None`.

#### Scenario: Preview after pane reshuffle stays visible

- **WHEN** the optimizer opens (reshuffling panes) and the user previews a candidate
- **THEN** the graph re-solves and the camera fits the live pane, so the reorder is visible without further input.

### Requirement: Arrangement-aware force sliders

The `S` sliders panel SHALL be available in every arrangement that has forces, and SHALL show the knobs of the active arrangement in the same three-row panel geometry:

- **Force arrangement**: repulsion gain, cohesion, and cable force (the existing tension, shared with `Alt+[`/`Alt+]`), so the user can shape the repulsion strength and the cluster cohesion live.
- **Sections arrangement**: slice force, section force, and cable force — unchanged in labels, ranges, defaults and behaviour.

Each row SHALL show its label and numeric value, and dragging a row SHALL re-solve the layout immediately with the new value and repaint, as it does in the sections arrangement. The panel rect SHALL consume the pointer ahead of node drag, marquee, selection and the minimap, so dragging a slider never grabs a node.

The column arrangement has no forces; `S` SHALL NOT lay out a panel there, and the status line SHALL report that the arrangement has no tunable forces instead.

#### Scenario: Force arrangement shows its own knobs

- **WHEN** the force arrangement is active and the user presses `S`
- **THEN** the panel shows Repulsion, Cohesion and Cable rows with their current values
- **AND** dragging Repulsion or Cohesion re-solves the force layout live and repaints

#### Scenario: Cable row mirrors the tension keybinding

- **WHEN** the force arrangement is active and the user adjusts the Cable row
- **THEN** the value is the same tension `Alt+[`/`Alt+]` adjusts, and both stay in sync

#### Scenario: Sections rows are unchanged

- **WHEN** the sections arrangement is active and the user presses `S`
- **THEN** the panel shows Slice, Section and Cable with the same ranges, defaults and live re-solve behaviour as before

#### Scenario: Column arrangement reports no forces

- **WHEN** the column arrangement is active and the user presses `S`
- **THEN** no panel is laid out and the status line reports that the column arrangement has no tunable forces

#### Scenario: Arrange follows the active arrangement

- **WHEN** the user activates Arrange (the panel button or `Shift+A`) in the force arrangement
- **THEN** the force layout is re-solved from its seed with the current knobs, the camera frames the result, and the status line reports the force arrangement

#### Scenario: Arrange keeps annealing in sections mode

- **WHEN** the user activates Arrange in the sections arrangement
- **THEN** the joint solve re-runs from the current positions (anneal) and reports the sections arrangement, as before
