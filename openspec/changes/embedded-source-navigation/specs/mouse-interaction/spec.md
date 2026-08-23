# Mouse Interaction Specification (delta)

## ADDED Requirements

### Requirement: Click selects component
The system SHALL set the clicked component as the explicit selection when the user clicks on it, in addition to the existing toggle behavior. Hover highlight remains distinct from selection.

#### Scenario: Click toggles and selects
- **WHEN** the user clicks a component
- **THEN** its state toggles (existing behavior) and it becomes the selected component

#### Scenario: Selection visible while hovering elsewhere
- **WHEN** a component is selected and the mouse moves over a different component
- **THEN** the hover highlight follows the cursor while the selection stays on the originally clicked component

### Requirement: Click empty panel space clears selection
The system SHALL clear the selection when the user clicks inside a panel area but outside any component rect, without moving the source pane position.

#### Scenario: Empty-space click deselects
- **WHEN** a component is selected and the user clicks panel background space
- **THEN** the selection is cleared and highlights are removed
- **AND** the source scroll position is unchanged

#### Scenario: Component click does not deselect
- **WHEN** a component is selected and the user clicks a different component
- **THEN** selection moves to the clicked component (no clear-then-set flicker)

### Requirement: Minimap click scrolls source
While the source pane is open, clicking within the minimap SHALL scroll the source to the document position under the click and update the viewport indicator.

#### Scenario: Click maps proportionally to source line
- **WHEN** the user clicks the minimap at 50% of its height for a 100-line file
- **THEN** the source scrolls to approximately line 50

#### Scenario: Click ignored without source pane
- **WHEN** the source pane is closed and the user clicks where the minimap would be
- **THEN** no source scroll occurs (the minimap does not exist in the layout)
