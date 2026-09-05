# Keybinding Specification

## Purpose

Vim-style prefix key system for the source viewer. The `g` key arms a prefix
mode with a lazy timeout; `g` + `v` opens the source viewer. While the viewer
is open, dedicated keys control scrolling, jumping, and closing.

## Requirements

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

#### Scenario: Non-matching key while prefix armed
Given prefix is armed
When user presses `h`
Then the prefix is cleared
And `h` processes as a normal key event

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

### Requirement: Signal-flow graph shortcut
The system SHALL open the signal-flow graph view via `g` + `g`: while the prefix is armed, pressing `g` clears the prefix and opens the graph view over the current patch (building the graph model, running the layout solver to convergence, and rendering nodes/edges/clusters). The sequence reuses the existing prefix mechanism (lazy 1 s timeout, `Esc` cancels the armed prefix unchanged). The graph view replaces the panel/source layout while open; `Esc` closes it and returns to the previous view state (controller panels, with any source-pane state preserved).

#### Scenario: Open graph with g g
Given no prefix is armed
And a patch is loaded
When user presses `g` then `g`
Then the signal-flow graph view opens showing circuits as nodes and virtual cables as edges
And `app.prefix` is `None`
And the layout solver has run to convergence and frozen node positions

#### Scenario: Open graph without patch
Given no patch is loaded
When user presses `g` then `g`
Then the graph view opens showing the empty-patch message

#### Scenario: Close graph with Esc
Given the graph view is open
When user presses `Esc`
Then the graph view closes
And focus returns to the controller panels
And any selected component and source-pane scroll state stay unchanged

#### Scenario: Prefix keys remain distinct
Given prefix is armed
When user presses `g` then `v`
Then the source viewer opens (not the graph)
When user presses `g` then `g`
Then the graph view opens (not the source viewer)

#### Scenario: Existing keybindings preserved
All existing keybindings SHALL remain functional and unchanged while the graph view is closed; opening the graph view SHALL NOT alter shift group, scale, orientation, or picker state.

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

### Requirement: Live panel interaction while viewer is open
While the source pane is open, component toggles (Enter/Space/click), shift-group changes (`1`–`4`), scale (`+`/`-`), and orientation (`o`) SHALL work regardless of viewer focus. Only conflicting navigation keys (`j`/`k`, Up/Down/Home/End) are routed by `ViewerFocus`; `Tab` switches focus.

#### Scenario: Panel keys work while source focused
Given viewer is open and source is focused
When user presses a digit, `+`, `o`, or Space on a hovered component
Then the shift group / scale / orientation / component state changes accordingly
And selecting a component scrolls the source view to its first occurrence

#### Scenario: Mouse click routes focus
Given viewer is open and source is focused
When the user left-clicks a component rect
Then the component toggles and focus becomes Panels
When the user left-clicks inside the source pane area
Then focus becomes Source without toggling anything or clearing the selection

### Requirement: Esc cancels prefix without clearing shift group
`Esc` while the prefix is armed (and the viewer is closed) SHALL cancel the prefix without other side effects; it does not clear the active shift group.

#### Scenario: Cancel prefix with Escape
Given prefix is armed
And shift group 3 is active
When user presses `Esc`
Then `app.prefix` becomes `None`
And `app.active_shift` remains `Some(Group3)`

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

### Requirement: Split ratio keys
When the source viewer is open, the `[` and `]` keys SHALL adjust the panels | source split ratio.

#### Scenario: Widen source
- **WHEN** the user presses `]` while the source viewer is open
- **THEN** the source pane widens by 10% and the panels column shrinks accordingly

#### Scenario: Narrow source
- **WHEN** the user presses `[` while the source viewer is open
- **THEN** the source pane narrows by 10% and the panels column grows accordingly

#### Scenario: Ratio clamped
- **WHEN** the user adjusts past either bound
- **THEN** the ratio stops at 30% / 70% for either side

#### Scenario: Viewer closed
- **WHEN** the source viewer is not open
- **THEN** pressing `[` or `]` has no effect on layout state

### Requirement: p toggles global processing pause

`p` SHALL toggle global processing pause from anywhere outside the picker (mirroring the global `q`/`l` keys), each toggle producing a status message.

#### Scenario: p pauses globally

- **WHEN** the user presses `p` with processing enabled
- **THEN** processing pauses and the status bar shows the paused state.

#### Scenario: p resumes

- **WHEN** the user presses `p` with processing paused
- **THEN** processing resumes.

### Requirement: x toggles hovered circuit processing in graph

`x` SHALL toggle processing for the hovered graph node's circuit instance while the graph surface is open, rebuilding the graph and recomputing influence. With no node hovered it is a no-op with no status message.

#### Scenario: x acts on hovered node

- **WHEN** the graph surface is open and a node is hovered
- **THEN** `x` toggles that circuit instance's processing state.

#### Scenario: x without hover is a no-op

- **WHEN** the graph surface is open and no node is hovered
- **THEN** `x` changes nothing.

### Requirement: Physical-view navigation keys

The system SHALL provide keys to pan the physical view when the rack overflows the terminal, to change zoom, and to toggle the skeleton reference mode.

#### Scenario: Pan keys move the viewport

- **WHEN** the rack overflows the terminal and the user presses a pan key
- **THEN** the viewport offset moves in the corresponding direction without changing zoom.

#### Scenario: Skeleton toggle switches presentation

- **WHEN** the user presses the skeleton-toggle key
- **THEN** the main view switches between full and skeleton presentation of the same layout, and back.

## Design Decisions

- Decision 1: Lazy timeout check (no background timer). Rationale: the app is event-driven; checking expiry on the next keypress avoids threading complexity and keeps the event loop simple. A stale prefix that nobody presses is harmless.
- Decision 2: `Esc` cancels prefix without clearing shift group. Rationale: shift group activation is an independent concern from prefix mode. Cancelling a mistaken `g` press should not disturb an active shift view.
- Decision 3: `g` + `v` chosen for viewer (not `g` + `s` or `g` + `p`). Rationale: `v` is mnemonic for "viewer" and avoids collision with potential future `g` + `s` (save/search) bindings.
- Decision 4: Scroll uses `u16` saturating arithmetic. Rationale: prevents underflow on decrement at 0, avoids panic on overflow at max.

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
