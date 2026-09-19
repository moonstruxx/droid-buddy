## MODIFIED Requirements

### Requirement: Pan and zoom navigation

The graph surface SHALL provide pan and zoom so the user can inspect a large layout at a legible scale. Zoom SHALL be driven by the mouse wheel (`+`/`-` step a preset scale) and pan by arrow keys or wheel-scroll on an overflowing layout, reusing the existing physical-view camera model (zoom preset + pan offset). The initial camera SHALL fit the graph against the actual visible pane (the graph slot rect in the tiled layout, the window canvas in the graph window), preserving aspect ratio and preferring to fill the canvas width, such that the smallest node renders at a readable width. The zoom presets SHALL extend at least 2× below the fitted zoom, so zooming out from the fit reaches at least half the fitted scale. A zoom step SHALL be anchored at the visible pane's center.

#### Scenario: Zoom to legible scale

- **WHEN** a large patch's graph is spread beyond the available width and the user presses `+`
- **THEN** the view zooms in so nodes render larger, and the graph image is re-transmitted at the new scale

#### Scenario: Pan an overflowing graph

- **WHEN** the graph overflows the main area and the user presses an arrow key or scrolls
- **THEN** the view pans in that direction and the graph image is re-transmitted at the new offset

#### Scenario: Legible initial fit

- **WHEN** a graph opens without user pan/zoom
- **THEN** the camera frames the graph against the visible pane's actual size (not a fixed viewport), preserving aspect ratio and preferring to fill the canvas width, so the smallest node still renders at a readable width (no node collapses to 1–2 characters)

#### Scenario: Zoom-out reaches below half the fitted zoom

- **WHEN** the graph is at the fitted zoom and the user presses `-` repeatedly
- **THEN** the zoom steps below half the fitted scale, and each step is anchored at the visible pane's center so the content under the center stays put

#### Scenario: Zoom anchor uses the visible pane center

- **WHEN** the graph pane is smaller than the app window and the user presses `+` or `-`
- **THEN** the zoom is anchored at the graph pane's center, not the app window's center or the world origin

### Requirement: Center and fit-and-center keys

The graph surface SHALL provide `c` to center the graph: pan so the drawn content's center lands at the visible pane's center, keeping the current zoom. `Shift+c` SHALL refit and center: re-frame the whole graph against the visible pane's actual size, replacing the current zoom. Both keys act on the graph surface when it has focus; with no graph open or before the first render they are no-ops with no status change. The help modal SHALL document both keys.

#### Scenario: Center keeps zoom

- **WHEN** the graph pane is focused, the user has panned or zoomed, and presses `c`
- **THEN** the camera pans so the graph content's center aligns with the visible pane's center, and the zoom is unchanged

#### Scenario: Fit and center reframes the visible pane

- **WHEN** the graph pane is focused and the user presses `Shift+c`
- **THEN** the camera refits to frame the whole graph against the visible pane's actual size, centered in that pane

#### Scenario: No graph open

- **WHEN** the user presses `c` or `Shift+c` with no graph open
- **THEN** nothing changes and no status message appears

## ADDED Requirements

### Requirement: Latency coloring toggle on the g-prefix

The latency coloring toggle SHALL live on the `g c` chord while the graph surface is open, matching the other graph chords (`g g`, `g v`, `g d`, `g o`, `g s`), instead of the bare `c` key. Bare `c` is reserved for centering. The toggle's effect is unchanged: it flips the latency ramp on/off for cable coloring and reports the new state in the status line.

#### Scenario: Toggle latency coloring with g c

- **WHEN** the graph surface is open and the user presses `g` then `c`
- **THEN** latency coloring toggles and the status line reports `Latency coloring on/off`

#### Scenario: Bare c does not toggle latency coloring

- **WHEN** the graph surface is open and the user presses bare `c`
- **THEN** the graph centers and latency coloring state is unchanged