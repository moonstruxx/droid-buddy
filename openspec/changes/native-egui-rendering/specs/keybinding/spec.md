## REMOVED Requirements

### Requirement: GPU window shortcut
**Reason**: `g w` opened an optional window with a `gui`-feature gate. The window is now always present, so there is no separate window to open or close.
**Migration**: Remove the `g w` binding.

### Requirement: Narrow-terminal carousel
**Reason**: There is no narrow-terminal collapse.
**Migration**: Remove.

## MODIFIED Requirements

### Requirement: Signal-flow graph shortcut
The system SHALL open the signal-flow graph view via `g` + `g`: while the prefix is armed, pressing `g` clears the prefix and opens the graph view over the current patch (building the graph model, running the layout solver to convergence, and rendering nodes, edges, and clusters). The graph view renders in the native window.

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
And any selected component state stays unchanged

#### Scenario: Prefix keys remain distinct
Given prefix is armed
When user presses `g` then `v`
Then the source viewer opens (not the graph)
When user presses `g` then `g`
Then the graph view opens (not the source viewer)

#### Scenario: Existing keybindings preserved
All existing keybindings SHALL remain functional and unchanged while the graph view is closed; opening the graph view SHALL NOT alter shift group, scale, orientation, or picker state.

#### Scenario: g g opens the window when configured
Given the `gui` feature is always enabled and the `[gui] graph_window` toggle is removed
When user presses `g` then `g`
Then the graph surface opens in the native window, since the window is always present and there is no terminal tile
