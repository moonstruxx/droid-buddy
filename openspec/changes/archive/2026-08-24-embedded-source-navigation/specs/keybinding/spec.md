# Keybinding Specification (delta)

## MODIFIED Requirements

### Requirement: Source viewer shortcut
The system SHALL open the embedded source pane via `g` + `v`: while the prefix is armed, pressing `v` sets `app.showing_viewer = true`, clears the prefix, focuses the source pane, and initializes source state from the loaded patch. The initial position follows the selection rule (selected component's first occurrence, otherwise beginning of file).

#### Scenario: Open viewer with prefix
Given no prefix is armed
And a patch is loaded
When user presses `g` then `v`
Then `app.showing_viewer` becomes true
And the source pane is focused with source content from the patch
And `app.prefix` is `None`

#### Scenario: Open viewer without patch
Given no patch is loaded
When user presses `g` then `v`
Then the source pane opens showing the empty-patch message

### Requirement: Viewer scroll controls
While the source pane is open, `j` SHALL scroll the source area down one line and `k` SHALL scroll up one line, with saturating arithmetic at 0 and clamping at the bottom of the content.

#### Scenario: Scroll viewer saturates at zero
Given viewer is open with scroll at 0
When user presses `k` (up)
Then the scroll position remains 0 (saturating)
When user presses `j` (down) three times
Then the scroll position is 3

#### Scenario: Arrow keys are not line scroll
Given viewer is open and focused
When user presses Up or Down
Then occurrence navigation runs (not one-line scroll)

### Requirement: Viewer close
While the source pane is open, `Esc` SHALL close it (`app.showing_viewer = false`) and return focus to the panel area. Component selection is preserved; on reopen the initial-position rule applies again.

#### Scenario: Close viewer with Escape
Given viewer is open
When user presses `Esc`
Then `app.showing_viewer` becomes false
And focus returns to the panels
And any selected component stays selected

## ADDED Requirements

### Requirement: Occurrence navigation keys
While the source pane is focused, Up SHALL move to the previous occurrence of the selected component, Down to the next occurrence, Home to the first, and End to the last, saturating at the bounds.

#### Scenario: Up/Down step occurrences
Given a component with three occurrences selected and cursor on the first
When user presses Down
Then the second occurrence scrolls into view
When user presses Up
Then the first occurrence scrolls into view

#### Scenario: Home/End jump to bounds
Given a component with occurrences selected
When user presses Home
Then the first occurrence is in view
When user presses End
Then the last occurrence is in view

### Requirement: View mode toggle key
While the source pane is open, `t` SHALL toggle between raw and prettified view modes.

#### Scenario: Toggle view mode
Given viewer is open showing raw text
When user presses `t`
Then prettified circuit blocks are shown
When user presses `t` again
Then raw text is shown again

### Requirement: Focus switching key
While the source pane is open, `Tab` SHALL switch focus between the source pane and the panel area.

#### Scenario: Tab moves focus to panels
Given viewer is open and focused
When user presses `Tab`
Then the panel area is focused and component keys work again

#### Scenario: Tab returns focus to viewer
Given viewer is open and focus is on panels
When user presses `Tab`
Then the source pane is focused again

### Requirement: Viewer input isolation
While the source pane is open, component toggles (Enter/Space/click), shift-group changes (1–4), scale (`+`/`-`), and orientation (`o`) SHALL be ignored until the pane closes — except when focus is explicitly moved back to the panel area via `Tab`.

#### Scenario: Readonly until refocused or closed
Given viewer is open and focused
When user presses Space, a digit, or clicks a component
Then no component state changes
When user presses Tab and then Space
Then the hovered/targeted component toggles normally
