# Spec Delta

## ADDED Requirements

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
