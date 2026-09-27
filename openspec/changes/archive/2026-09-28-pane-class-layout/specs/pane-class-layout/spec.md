# Spec Delta

## Purpose

A class-based pane layout for the main band: a big pane on the left and a right region that holds either a second big pane or two small panes, two window classes that decide which pane a view may occupy, a non-latching maximize, and swap operations that exchange views between panes.

## ADDED Requirements

### Requirement: Adaptive pane layout

The main band SHALL render a big pane on the left half at full height. The right half SHALL take one of two arrangements. When no small-class view is open the right half SHALL hold a second big pane, so the band is two equal panes side by side. When at least one small-class view is open the right half SHALL split vertically into two small panes of a quarter of the window each. The horizontal boundary SHALL be adjustable with `[` and `]` and the vertical boundary between the two small panes with `Alt+[` and `Alt+]`, both clamped to 30 and 70 percent, with an even split as the default.

#### Scenario: No small-class view open
- **WHEN** no small-class view is open
- **THEN** the band shows two equal panes side by side, each able to hold a big-class view

#### Scenario: Small-class view open
- **WHEN** a small-class view is open
- **THEN** the left half holds the big pane and the right half splits vertically into two small panes of a quarter each

#### Scenario: Horizontal boundary adjusts
- **WHEN** the user presses `]` and then `[`
- **THEN** the left and right boundary moves in 10 percent steps and stops at the 30 and 70 percent limits

#### Scenario: Vertical boundary adjusts
- **WHEN** two small panes are shown and the user presses `Alt+]` and then `Alt+[`
- **THEN** the boundary between them moves in 10 percent steps and stops at the 30 and 70 percent limits

### Requirement: Window classes route views to panes

Every view SHALL belong to a window class. The Big class SHALL contain the signal-flow graph, the module UI (panels), and the physical rack view. The Small class SHALL contain the latency optimizer and the source viewer. A view SHALL open only in a pane of its own class. Opening a small-class view while the right half holds a big-class view SHALL close that big-class view and split the right half into the two small panes.

#### Scenario: Big-class view opens in a big pane
- **WHEN** the user opens the graph with `g g`
- **THEN** the graph appears in a big pane

#### Scenario: Small-class view opens in a small pane
- **WHEN** the user opens the optimizer with `g o`
- **THEN** the optimizer appears in a small pane and the left big pane keeps its view

#### Scenario: Opening a visible view focuses it
- **WHEN** the graph is already shown in a big pane and the user presses `g g`
- **THEN** focus moves to that pane without opening a second copy

#### Scenario: Occupied class pane is replaced
- **WHEN** the left big pane shows the module UI and the user opens the graph with `g g`
- **THEN** the graph replaces the module UI in that pane

#### Scenario: Right half repurposed for a small view
- **WHEN** two big panes are shown, the right one holds the graph, and the user opens the optimizer with `g o`
- **THEN** the graph closes, the right half splits into two small panes, and the optimizer occupies one of them

### Requirement: Physical view and module UI are mutually exclusive

The physical rack view and the module UI SHALL NOT be open at the same time. Opening either SHALL close the other wherever it is shown. A swap SHALL NOT leave both open.

#### Scenario: Opening physical closes module UI
- **WHEN** the module UI is open and the user opens the physical view
- **THEN** the module UI closes and the physical view takes its pane

#### Scenario: Opening module UI closes physical
- **WHEN** the physical view is open and the user opens the module UI
- **THEN** the physical view closes and the module UI takes its pane

#### Scenario: Swap does not create coexistence
- **WHEN** only the module UI of the pair is open and the user swaps panes
- **THEN** the physical view stays closed and only the module UI is shown

### Requirement: Startup pane configuration

On startup the layout SHALL open with the module UI in the left big pane and the source viewer in a small pane, so the band starts in the arrangement with two small panes.

#### Scenario: Startup layout
- **WHEN** the application starts with a patch loaded
- **THEN** the left big pane shows the module UI and a small pane shows the source viewer

#### Scenario: Second small pane starts empty
- **WHEN** the application starts and no second small-class view is opened
- **THEN** the second small pane renders empty

### Requirement: Focus routing across panes

Exactly one pane SHALL be focused. `Tab` SHALL move focus forward through the panes of the current arrangement in tree order and `Shift+Tab` SHALL move it backward, skipping empty panes. The focused pane's border SHALL use the `pane_focus_border` token and unfocused panes the `pane_unfocused_border` token. Keyboard input SHALL route to the focused pane's view. A mouse click inside a pane SHALL focus that pane.

#### Scenario: Tab cycles focus with two small panes
- **WHEN** the band shows the left big pane and two small panes, all holding views, and the user presses `Tab` repeatedly
- **THEN** focus moves through the left big pane, the first small pane, the second small pane, and back

#### Scenario: Tab cycles focus with two big panes
- **WHEN** the band shows two big panes, both holding views, and the user presses `Tab` repeatedly
- **THEN** focus moves between the left and right big panes

#### Scenario: Empty panes are skipped
- **WHEN** the second small pane is empty and the user presses `Tab` from the first small pane
- **THEN** focus returns to the left big pane

#### Scenario: Focus border marks the active pane
- **WHEN** a pane is focused
- **THEN** its border uses the `pane_focus_border` token and the other panes use `pane_unfocused_border`

#### Scenario: Keys route to the focused view
- **WHEN** the graph pane is focused and the user presses `h`
- **THEN** the graph layout mode toggles, and the same key does nothing when another view is focused

### Requirement: Non-latching maximize

`z` SHALL toggle the focused pane to fill the whole main band and back to its arrangement. Maximize SHALL NOT latch: pressing `z` again, moving focus to another pane, or pressing `Esc` SHALL restore the arrangement. While a pane is maximized the other panes SHALL not render.

#### Scenario: Maximize the focused pane
- **WHEN** a small pane is focused and the user presses `z`
- **THEN** that pane fills the main band and the other panes are hidden

#### Scenario: Restore the arrangement
- **WHEN** a pane is maximized and the user presses `z` again
- **THEN** the arrangement returns with the same views in the same panes

#### Scenario: Esc restores the arrangement
- **WHEN** a pane is maximized and the user presses `Esc`
- **THEN** the maximize clears and the arrangement returns

#### Scenario: Focus change restores the arrangement
- **WHEN** a pane is maximized and the user presses `Tab`
- **THEN** the maximize clears, focus moves to the next pane, and the arrangement returns

### Requirement: Same-class swap

`Alt+b` SHALL exchange the view in the focused pane with the view in the big pane that is not focused. When the focused pane is the only big pane, `Alt+b` SHALL report a no-op. `Alt+s` SHALL exchange the views of the two small panes and SHALL report a no-op when no small panes are shown. Both operations SHALL preserve focus on the pane the user was working in.

#### Scenario: Swap the two big panes
- **WHEN** two big panes are shown and the user presses `Alt+b`
- **THEN** the two panes exchange views

#### Scenario: Promote a small view to the big pane
- **WHEN** the left big pane shows the module UI, a small pane shows the graph, that small pane is focused, and the user presses `Alt+b`
- **THEN** the graph moves to the left big pane and the module UI moves to that small pane

#### Scenario: Swap big while the only big pane is focused
- **WHEN** two small panes are shown, the left big pane is focused, and the user presses `Alt+b`
- **THEN** the layout does not change and the status reports that no swap applies

#### Scenario: Swap the two small panes
- **WHEN** the first small pane shows the source viewer and the second shows the optimizer
- **THEN** pressing `Alt+s` moves the optimizer to the first small pane and the source viewer to the second

#### Scenario: Swap small with no small panes
- **WHEN** two big panes are shown and the user presses `Alt+s`
- **THEN** the layout does not change and the status reports that no swap applies

#### Scenario: Swap with an empty pane
- **WHEN** the second small pane is empty and the user presses `Alt+s`
- **THEN** the first small pane's view moves to the second pane and the first becomes empty

### Requirement: Esc closes the focused view

`Esc` SHALL close the view in the focused pane, leaving the pane empty. If the focused pane is maximized, `Esc` SHALL clear the maximize instead of closing the view. Closing the last small-class view SHALL return the right half to a single big pane. Closing a view SHALL not change the other panes.

#### Scenario: Close a view
- **WHEN** the optimizer is focused in a small pane and the user presses `Esc`
- **THEN** the optimizer closes and the pane becomes empty

#### Scenario: Closing the last small view restores the big right pane
- **WHEN** the source viewer is the only small-class view open and the user closes it
- **THEN** the right half returns to a single empty big pane

#### Scenario: Esc clears maximize first
- **WHEN** the graph is maximized in a big pane and the user presses `Esc`
- **THEN** the maximize clears and the graph stays open

### Requirement: Overlays render above the pane layout

The file picker (`l`), the validation modal (`e`), the select-state menu (`g s`), the label overlay, the diff surface, and the help modal (`?`) SHALL render as centered or full-surface overlays above the pane layout. They SHALL NOT become panes, and while one has focus it SHALL consume the pane layout keys.

#### Scenario: Picker renders above panes
- **WHEN** the user opens the file picker with `l`
- **THEN** the picker renders above the pane layout and the panes stay visible behind it

#### Scenario: Overlay consumes layout keys
- **WHEN** the help modal is open and the user presses `z`
- **THEN** the maximize does not toggle
