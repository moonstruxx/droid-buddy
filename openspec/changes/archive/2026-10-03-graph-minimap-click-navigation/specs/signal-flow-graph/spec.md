# Spec Delta

## MODIFIED Requirements

### Requirement: Pan and zoom navigation

The graph surface SHALL provide pan and zoom so the user can inspect a large layout at a legible scale. Zoom SHALL be driven by the mouse wheel (`+`/`-` step a preset scale) and pan by arrow keys, wheel-scroll on an overflowing layout, **or clicking the minimap**, reusing the existing physical-view camera model (zoom preset + pan offset). The initial camera SHALL fit the graph against the actual visible pane (the graph slot rect in the class layout, the window canvas in the graph window), preserving aspect ratio and preferring to fill the canvas width, such that the smallest node renders at a readable width. The zoom presets SHALL extend at least 2× below the fitted zoom, so zooming out from the fit reaches at least half the fitted scale. A zoom step SHALL be anchored at the visible pane's center.

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

#### Scenario: Minimap click pans camera to clicked location

- **WHEN** graph pane is open and minimap is visible, AND user left-clicks inside the minimap panel
- **THEN** graph camera pans so the clicked minimap position maps to the world center of the visible viewport, preserving current zoom level