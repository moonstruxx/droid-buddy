# Module Scaling Specification

## Purpose

Scale the rendering of modules and components. With the physical scale model, the `+`/`-` scale presets become physical zoom levels over the rack.

## MODIFIED Requirements

### Requirement: Component scaling presets

The system SHALL interpret the `+`/`-` scale presets as physical zoom levels over the rack: a uniform mm→characters factor applied to the whole physical layout, not per-cell size inflation.

#### Scenario: User selects 150% scaling

- **WHEN** user presses the `+` key to increase scaling
- **THEN** the whole rack zooms to 150% of its base mapping and the status bar displays "Scaling: 150%"

#### Scenario: User selects 50% scaling

- **WHEN** user presses the `-` key to decrease scaling below 100%
- **THEN** the whole rack zooms to 50% of its base mapping and the status bar displays "Scaling: 50%"

#### Scenario: Zoom scales the whole rack

- **WHEN** the user presses `+` or `-`
- **THEN** the entire physical layout scales uniformly around a fixed anchor, and component rects and hit rects follow the scaled cells.

#### Scenario: Minimum readability preserved

- **WHEN** the zoom would make element cells too narrow to render content
- **THEN** the cell content shrinks to fit or falls back to the unboxed rendering path, and the zoom never drops below the floor that keeps module cells boxable.