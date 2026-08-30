# Mouse Interaction Specification

## Purpose

Handle mouse events for hover, selection, and value adjustment. The physical scale model adds mouse-wheel pan/scroll over the rack.

## ADDED Requirements

### Requirement: Wheel pans the physical view

The system SHALL pan the physical view with the mouse wheel when the rack overflows the terminal, keeping the 1:1 mapping and hover/click hit-testing intact under the current pan offset and zoom.

#### Scenario: Wheel scrolls an overflowing rack

- **WHEN** the rack overflows the terminal and the user scrolls the wheel
- **THEN** the viewport pans in the wheel direction; a wheel on a knob/fader cell adjusts its value as before when no overflow forces panning.

#### Scenario: Hover and click follow the pan

- **WHEN** the view is panned
- **THEN** hover highlight and click hit-testing use the panned `component_rects`, matching the rendered cells.