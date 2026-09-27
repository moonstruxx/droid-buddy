# Spec Delta

## MODIFIED Requirements

### Requirement: Window classes route views to panes

Every view SHALL belong to a window class. The Big class SHALL contain the signal-flow graph and the module UI (the physical rack view). The Small class SHALL contain the latency optimizer and the source viewer. A view SHALL open only in a pane of its own class. Opening a small-class view while the right half holds a big-class view SHALL close that big-class view and split the right half into the two small panes.

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

#### Scenario: Big carousel has two views
- **WHEN** a big pane is focused and the user presses `r` repeatedly
- **THEN** the pane alternates between the graph and the module UI

### Requirement: Startup pane configuration

On startup the layout SHALL open with the module UI (the physical rack view) in the left big pane and the source viewer in a small pane, so the band starts in the arrangement with two small panes.

#### Scenario: Startup layout
- **WHEN** the application starts with a patch loaded
- **THEN** the left big pane shows the module UI and a small pane shows the source viewer

#### Scenario: Second small pane starts empty
- **WHEN** the application starts and no second small-class view is opened
- **THEN** the second small pane renders empty

## REMOVED Requirements

### Requirement: Physical view and module UI are mutually exclusive

**Reason**: The physical rack view is now the module UI, so there is no second view to exclude.

**Migration**: None; opening the module UI opens the physical rack view.
