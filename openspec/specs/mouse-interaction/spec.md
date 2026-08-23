# Mouse Interaction Specification

## Purpose

Enable mouse interaction within the TUI for clicking components to toggle state, hovering for visual highlight, and scrolling/dragging for knob and fader adjustment — working correctly inside terminal multiplexers like Herdr and tmux.

## Requirements

### Requirement: Enable mouse capture
The system SHALL enable crossterm mouse capture on startup and disable it on exit, ensuring clean terminal state restoration.

#### Scenario: Mouse enabled on startup
- **WHEN** the application starts
- **THEN** mouse capture is enabled and mouse events are received

#### Scenario: Mouse disabled on exit
- **WHEN** the application exits (via `q` or Ctrl+C)
- **THEN** mouse capture is disabled and terminal state is restored

### Requirement: Click to toggle button state
The system SHALL toggle a button or switch component's state when the user clicks on it.

#### Scenario: Click button to toggle on
- **WHEN** the user clicks on a button component that is OFF
- **THEN** the button state changes to ON and the visual indicator updates

#### Scenario: Click button to toggle off
- **WHEN** the user clicks on a button component that is ON
- **THEN** the button state changes to OFF and the visual indicator updates

### Requirement: Hover highlight
The system SHALL highlight the component under the mouse cursor with a distinct visual style (e.g., reversed colors or background tint).

#### Scenario: Mouse hover over component
- **WHEN** the mouse cursor moves over a component
- **THEN** that component is visually highlighted

#### Scenario: Mouse moves away
- **WHEN** the mouse cursor moves away from a component
- **THEN** the highlight is removed

### Requirement: Scroll to adjust knob/fader values
The system SHALL increment or decrement a knob or fader component's value when the user scrolls the mouse wheel over it.

#### Scenario: Scroll up on knob
- **WHEN** the user scrolls up while hovering over a knob component
- **THEN** the knob's value increases by a step (e.g., +0.05)

#### Scenario: Scroll down on knob
- **WHEN** the user scrolls down while hovering over a knob component
- **THEN** the knob's value decreases by a step (e.g., -0.05)

### Requirement: Multiplexer compatibility
The system SHALL work correctly inside Herdr and tmux when mouse mode is enabled in the multiplexer.

#### Scenario: Click works in Herdr
- **WHEN** the app runs inside a Herdr pane with mouse enabled
- **THEN** click events are correctly received and processed

#### Scenario: Click works in tmux
- **WHEN** the app runs inside a tmux pane with `set -g mouse on`
- **THEN** click events are correctly received and processed

### Requirement: Keyboard navigation preserved
The system SHALL preserve all existing keyboard navigation (j/k, Enter/Space, shift keys 1-4, Esc, q) alongside mouse interaction.

#### Scenario: Keyboard and mouse both work
- **WHEN** the user alternates between keyboard and mouse input
- **THEN** both input methods work correctly without interference

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