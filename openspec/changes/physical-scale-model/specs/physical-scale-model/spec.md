# Physical Scale Model Specification

## Purpose

Render a patch's hardware as a 1:1 physical scale model of the DROID master + controller chain: modules at real sizes (mm derived from HP), elements at real faceplate positions, in the order the patch declares them. A geometry-only skeleton render serves as the validation reference; the full render must coincide with it.

## ADDED Requirements

### Requirement: Physical grid model

The system SHALL model a patch's physical layout as an ordered chain of controller modules with real millimeter geometry — module width derived from HP (1 HP = 5.08 mm), element cells with position and size per element family — derived from embedded per-controller geometry data, in the order the patch declares the controllers.

#### Scenario: Chain order matches patch declaration

- **WHEN** a patch declares controllers in a given order
- **THEN** the physical layout places their modules left-to-right in that order.

#### Scenario: Repeated instances become separate faceplates

- **WHEN** a patch declares two instances of the same controller
- **THEN** each instance is rendered as its own module at its real width, placed in declaration order.

#### Scenario: Element cells resolve per token

- **WHEN** a hardware token (B/P/S/E/I/O/L family) belongs to a controller
- **THEN** its element cell position and size resolve from that controller's geometry data.

### Requirement: Skeleton reference render

The system SHALL provide a geometry-only render mode drawing only module outlines and element cells — the important visual characteristics — without labels or states, as the validation reference.

#### Scenario: Skeleton shows structure only

- **WHEN** skeleton mode is active
- **THEN** the render shows module rectangles and element cells, with no component labels or state.

#### Scenario: Skeleton is toggleable

- **WHEN** the user presses the skeleton toggle
- **THEN** the render switches between full and skeleton presentation of the same layout.

### Requirement: 1:1 main view

The system SHALL render components onto the physical grid cells as the main view: a uniform mm→characters mapping with aspect compensation, zoom levels, and pan/scroll for racks exceeding the terminal area; hit-testing publishes `component_rects` matching the rendered cells.

#### Scenario: Components land on their grid cells

- **WHEN** the full view renders
- **THEN** every rendered component rect equals its grid-model cell under the same scale and offset.

#### Scenario: Overflow pans

- **WHEN** the rack is wider or taller than the terminal main area
- **THEN** the view pans/scrolls to reveal the rest.

#### Scenario: Zoom scales the rack

- **WHEN** the user changes zoom
- **THEN** the whole rack scales uniformly around a fixed anchor.

### Requirement: Coincidence verification

The system SHALL verify the full render against the skeleton: tests assert every full-render element rect coincides with its skeleton cell, and the gallery matrix renders skeleton | full side by side.

#### Scenario: Full render coincides with skeleton

- **WHEN** both renders are produced for the same patch at the same viewport
- **THEN** every full-render element rect equals the corresponding skeleton cell.

#### Scenario: Gallery proves fidelity

- **WHEN** the gallery matrix is generated
- **THEN** each physical-layout scenario includes a skeleton row and a full row for side-by-side proof.