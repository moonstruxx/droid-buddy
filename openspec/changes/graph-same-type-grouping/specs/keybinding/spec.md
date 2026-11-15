# Spec Delta

## ADDED Requirements

### Requirement: Graph grouping keys

The graph pane SHALL bind `o` (no modifiers) to isolate the hovered or selected group, `Enter` to expand or collapse the hovered group, and `Shift+O` to toggle grouping for the whole graph. `o` and `Enter` SHALL be silent no-ops when no group is hovered or selected. The `g o` prefix chord SHALL keep opening the optimizer and SHALL NOT be shadowed by the bare `o` binding. The help modal's graph table and the user manual SHALL list all three keys.

#### Scenario: Help documents the grouping keys

- **WHEN** the user opens the graph help view
- **THEN** the key table lists `o` (isolate group), `Enter` (expand or collapse group), and `Shift+O` (toggle grouping)

#### Scenario: Isolate without a group is inert

- **WHEN** the user presses `o` with no group hovered or selected
- **THEN** nothing changes and no status message is reported

#### Scenario: The g o chord still opens the optimizer

- **WHEN** the g-prefix is armed and the user presses `o`
- **THEN** the optimizer pane opens as before, and the isolate binding does not fire

#### Scenario: Expansion without a group is inert

- **WHEN** the user presses `Enter` with no group hovered
- **THEN** nothing changes and no status message is reported
