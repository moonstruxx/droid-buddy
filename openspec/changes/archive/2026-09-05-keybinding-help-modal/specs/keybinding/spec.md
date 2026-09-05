## ADDED Requirements

### Requirement: Help modal keybinding

The system SHALL open a floating help modal on `?` showing the keybindings for the current active view. The modal SHALL close on `Esc`, on `q` (without quitting the app), or on a mouse click outside the modal.

#### Scenario: Open help from the panels view
- **WHEN** the user presses `?` in the main panels/physical view
- **THEN** a floating help modal opens listing the main-view keybindings

#### Scenario: Help content follows the active view
- **WHEN** the user presses `?` while the graph surface is open
- **THEN** the help modal lists the graph-surface keybindings

#### Scenario: Esc closes help
- **WHEN** the help modal is open and the user presses `Esc`
- **THEN** the modal closes and the underlying view is unchanged

#### Scenario: q closes help without quitting
- **WHEN** the help modal is open and the user presses `q`
- **THEN** the modal closes and the app does not quit

#### Scenario: Click outside closes help
- **WHEN** the help modal is open and the user clicks outside the modal
- **THEN** the modal closes

#### Scenario: Click inside keeps help open
- **WHEN** the help modal is open and the user clicks inside the modal
- **THEN** the modal stays open