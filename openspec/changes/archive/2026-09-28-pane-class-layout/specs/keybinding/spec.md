# Keybinding Specification (Delta)

## ADDED Requirements

### Requirement: Pane layout keys

The system SHALL bind `z` to toggle the maximize of the focused pane, `Alt+b` to exchange the big pane's view with the focused pane's view, and `Alt+s` to exchange the two small panes. These keys SHALL apply in the main band and SHALL NOT fire while the file picker, the validation modal, the label overlay, or the help modal has focus. The help modal tables SHALL list all three keys.

#### Scenario: z toggles maximize
- **WHEN** the user presses `z`
- **THEN** the focused pane maximizes, and pressing `z` again restores the split

#### Scenario: Alt+b swaps big with the focused pane
- **WHEN** the user presses `Alt+b` with a small pane focused
- **THEN** the big pane's view and the focused pane's view exchange places

#### Scenario: Alt+s swaps the small panes
- **WHEN** the user presses `Alt+s`
- **THEN** the two small panes exchange views

#### Scenario: Overlays take priority
- **WHEN** the help modal is open and the user presses `z`
- **THEN** the maximize does not toggle and the modal handles the key

#### Scenario: Help lists the layout keys
- **WHEN** the user opens help with `?`
- **THEN** the table lists `z`, `Alt+b`, and `Alt+s`
