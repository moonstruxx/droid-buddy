## ADDED Requirements

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
