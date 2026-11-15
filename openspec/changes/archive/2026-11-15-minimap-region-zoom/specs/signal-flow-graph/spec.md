# Spec Delta

## ADDED Requirements

### Requirement: Minimap region zoom and always-visible map

The signal-flow graph SHALL draw its minimap whenever it has a scene, not only when the world overflows the viewport; a scene that fits the canvas SHALL paint a viewport box clamped to the map's inner area that reads as "the whole scene is visible".

A press inside the minimap SHALL anchor a marquee rather than moving the camera. While the pointer is down the marquee SHALL track the pointer, clamped to the map's inner area, and SHALL be drawn as a translucent accent fill with a one-pixel accent stroke. On release the gesture SHALL zoom the graph to fit the world rectangle under the marquee; when the marquee spans fewer than three pixels on either axis it SHALL instead pan the camera to the anchored world point, unchanged from a plain click.

The fit SHALL use the graph's existing camera fit over the selected world rectangle, and SHALL latch the fit zoom preset. `Esc` SHALL cancel an active marquee without closing the graph. While the minimap owns the drag, the canvas SHALL NOT start a marquee selection, and the gesture SHALL NOT fall through to node selection or node drag.

The minimap↔world mapping SHALL have one implementation, shared by the pointer path and the painter.

#### Scenario: Drag a rectangle to zoom

- **WHEN** the user presses inside the minimap and releases after dragging a marquee larger than three pixels on both axes
- **THEN** the camera is fitted to the world rectangle under that marquee and the fit zoom preset is latched

#### Scenario: A short drag still pans

- **WHEN** the user presses and releases inside the minimap with a marquee under three pixels on at least one axis
- **THEN** the camera pans so the anchored world point becomes the viewport centre at the current zoom

#### Scenario: The marquee is visible while dragging

- **WHEN** the user is dragging inside the minimap
- **THEN** the marquee is painted over the map's dots, clamped to the map's inner area, and no canvas selection marquee starts

#### Scenario: Escape cancels a marquee

- **WHEN** the user presses `Esc` while dragging in the minimap
- **THEN** the marquee is discarded, the camera is unchanged, and the graph stays open

#### Scenario: The map is drawn at every zoom

- **WHEN** the graph's world fits the canvas
- **THEN** the minimap is still drawn, with a viewport box covering the map's inner area

#### Scenario: An empty scene has no map

- **WHEN** the graph has no nodes
- **THEN** no minimap is laid out and minimap gestures are inert
