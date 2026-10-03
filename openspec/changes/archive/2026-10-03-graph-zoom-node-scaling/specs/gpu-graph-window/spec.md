# Spec Delta

## MODIFIED Requirements

### Requirement: Canvas polish

The GPU window SHALL provide smooth pan and zoom under the shared `GraphCamera`, marquee selection over multiple nodes, a minimap of the full layout, and a hover tooltip showing the circuit's latency readouts. Pan and zoom SHALL scale the drawn node geometry with the camera — node frames, titles, ports, cable strokes, and cluster chrome — and pointer hit-testing SHALL stay aligned with the drawn geometry at every zoom, including the level-of-detail case where titles and ports are omitted.

#### Scenario: Pan and zoom the canvas

- **WHEN** the user scrolls or drags the window canvas
- **THEN** the view pans and zooms smoothly and hit-testing stays aligned with the drawn geometry

#### Scenario: Marquee selection

- **WHEN** the user drags a selection rectangle across several nodes
- **THEN** all enclosed nodes become selected and the selection propagates to the other views

#### Scenario: Minimap

- **WHEN** the graph is larger than the window viewport
- **THEN** a minimap shows the full layout with a viewport indicator

#### Scenario: Zoom scales drawn geometry

- **WHEN** the user zooms the window canvas
- **THEN** node frames and cable strokes scale with the zoom, titles drop out once frames fall below the legibility threshold, and hit-testing stays aligned with what is drawn
