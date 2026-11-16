# Spec Delta

## ADDED Requirements

### Requirement: Module UI minimap

The module UI pane SHALL draw a minimap whenever it has a rack, including when the whole rack already fits the pane, so the map is a persistent navigation aid rather than an overflow indicator. The map SHALL show the rack case outline, one rect per module faceplate, the focused element's module distinguished from the rest, and a viewport box marking the part of the rack the pane currently displays; when the rack fits, that box SHALL be clamped into the map's inner area and read as "the whole rack is visible".

The map SHALL map the rack's extent at the current zoom with one isotropic scale, so the map and the pane's own overflow judgement agree about where the rack ends, and it SHALL be positioned inside the pane without covering the rack's top-left corner.

A press inside the minimap panel SHALL be claimed by the map before element hit-testing, so a click on the map SHALL NOT focus an element and a wheel over it SHALL NOT adjust a value. A click SHALL centre the pane on the rack point under the pointer. A drag of three points or more SHALL fit the zoom to the dragged region. `Esc` SHALL cancel an active marquee without closing the pane. The minimap SHALL NOT be drawn in the performance view, which is already the compact representation of the rack.

#### Scenario: The map is drawn at the labels fit

- **WHEN** a rack is loaded and the pane opens at the labels fit, which overflows the pane
- **THEN** the minimap is drawn with the case outline, a rect per module and a viewport box marking the visible quarter of the rack

#### Scenario: The map is drawn when the rack fits

- **WHEN** the pane is zoomed so the whole rack fits
- **THEN** the minimap is still drawn, and its viewport box is clamped inside the map and reads as the whole rack being visible

#### Scenario: A click centres on the clicked place

- **WHEN** the pointer clicks a module rect in the minimap
- **THEN** the pane centres on that module's rack position at the current zoom, and the map's viewport box moves with it

#### Scenario: A drag frames the dragged region

- **WHEN** the pointer drags at least three points across the minimap
- **THEN** the pane's zoom is fitted to the rack region the marquee covered, and a marquee shorter than three points on both axes stays a click

#### Scenario: Escape cancels a marquee

- **WHEN** a marquee is in progress and `Esc` is pressed
- **THEN** the marquee is cleared and the pane stays open with its zoom unchanged

#### Scenario: The map does not steal element gestures

- **WHEN** the pointer presses inside the minimap panel over a spot where an element cell also lies
- **THEN** no element is focused and no value is adjusted, and the gesture is the map's

#### Scenario: No map in the performance view

- **WHEN** the performance view is showing
- **THEN** no minimap is drawn, because that view already presents the whole rack compactly
