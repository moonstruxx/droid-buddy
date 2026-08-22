# Keybinding Specification

## Purpose

Define keyboard shortcuts for navigating and interacting with the DROID TUI, including patch loading, component control, shift groups, and module resizing.

## MODIFIED Requirements

### Requirement: Prefix key pattern for extensibility
The system SHALL use a prefix key pattern (`g` + action key) for extended commands to allow future expansion without conflicts. Single-key bindings remain for frequent actions.

#### Scenario: G prefix activation
- **WHEN** user presses `g`
- **THEN** the system enters prefix mode with status indicator "Prefix: g"

#### Scenario: Prefix timeout
- **WHEN** user presses `g` then waits more than 1 second without a second key
- **THEN** the prefix mode times out and returns to normal mode

#### Scenario: Esc cancels prefix
- **WHEN** user presses `g` then `Esc`
- **THEN** the prefix mode is cancelled immediately

### Requirement: Resize mode activation
The system SHALL provide a resize mode activated via the `g r` key sequence. Resize mode allows module dimension adjustment via arrow keys.

#### Scenario: Enter resize mode
- **WHEN** user presses `g` then `r`
- **THEN** the system enters resize mode with status "RESIZE MODE (proportional)"

#### Scenario: Resize mode with free proportion
- **WHEN** user holds Shift while pressing `g r`
- **THEN** the system enters resize mode with status "RESIZE MODE (free)"

#### Scenario: Exit resize mode
- **WHEN** user presses `Esc` in resize mode
- **THEN** the system exits resize mode and returns to normal mode

### Requirement: Arrow key resize controls
While in resize mode, arrow keys SHALL adjust the selected module's dimensions. The behavior depends on whether proportional or free proportion mode is active.

#### Scenario: Right arrow in proportional mode
- **WHEN** user presses Right arrow in proportional resize mode
- **THEN** both width and height increase by one unit maintaining aspect ratio

#### Scenario: Left arrow in proportional mode
- **WHEN** user presses Left arrow in proportional resize mode
- **THEN** both width and height decrease by one unit (not below minimum)

#### Scenario: Up/Down arrows in free proportion mode
- **WHEN** user presses Up/Down arrows in free proportion mode
- **THEN** only height increases/decreases without affecting width

#### Scenario: Left/Right arrows in free proportion mode
- **WHEN** user presses Left/Right arrows in free proportion mode
- **THEN** only width decreases/increases without affecting height

### Requirement: Existing keybindings preserved
All existing keybindings SHALL remain functional and unchanged. New resize mode bindings SHALL NOT conflict with existing shortcuts.

#### Scenario: Legacy navigation works
- **WHEN** user presses `j`/`k`/arrows outside resize mode
- **THEN** component navigation functions as before

#### Scenario: Legacy toggle works
- **WHEN** user presses Enter/Space or clicks on a component outside resize mode
- **THEN** component toggling functions as before

#### Scenario: Legacy picker works
- **WHEN** user presses `l` outside resize mode
- **THEN** file picker opens as before

### Requirement: Status bar mode indication
The system SHALL display the current mode in the status bar to provide clear feedback about available keybindings.

#### Scenario: Normal mode indicator
- **WHEN** in normal mode with no patch loaded
- **THEN** status shows "Press 'l' to load a patch"

#### Scenario: Resize proportional mode indicator
- **WHEN** in resize mode (proportional)
- **THEN** status shows "RESIZE MODE (proportional) - Arrows: size, Esc: cancel"

#### Scenario: Resize free mode indicator
- **WHEN** in resize mode (free proportion)
- **THEN** status shows "RESIZE MODE (free) - Arrows: width/height, Shift+Arrows: opposite dim, Esc: cancel"
