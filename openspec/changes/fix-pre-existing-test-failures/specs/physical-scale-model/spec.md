# physical-scale-model Specification (delta)

## MODIFIED Requirements

### Requirement: 1:1 main view

The system SHALL render components onto the physical grid cells as the main view: a uniform mm→characters mapping with aspect compensation, zoom levels, and pan/scroll for racks exceeding the terminal area; hit-testing publishes `component_rects` matching the rendered cells. Module borders MUST abut without overlapping: the mm→chars mapping and cell-rect rounding MUST NOT cause adjacent module borders to share or cross a column at any supported zoom level.

#### Scenario: Components land on their grid cells

- **WHEN** the full view renders
- **THEN** every rendered component rect equals its grid-model cell under the same scale and offset.

#### Scenario: Overflow pans

- **WHEN** the rack is wider or taller than the terminal main area
- **THEN** the view pans/scrolls to reveal the rest.

#### Scenario: Zoom scales the rack

- **WHEN** the user changes zoom
- **THEN** the whole rack scales uniformly around a fixed anchor.

#### Scenario: Fold bar shows row structure

- **WHEN** the rack has more than one row
- **THEN** the case outline wraps the whole rack and a fold-bar divider is rendered at each row boundary, in both skeleton and full presentation.

#### Scenario: Adjacent module borders never overlap

- **WHEN** two modules are placed adjacently in the same row at any supported zoom level
- **THEN** their rendered borders abut exactly — the right border of the left module never shares or crosses the left border of the right module.

### Requirement: Element state rendering

The system SHALL render each element's state on its physical-view cell: buttons/switches show their toggle glyph, knobs/encoders/faders show their percentage, and CV I/O shows its direction — mirroring the panel view's state rendering, so the physical view is a faithful 1:1 representation of the patch's live state.

#### Scenario: Switch state renders on the physical view

- **WHEN** a patch declares a switch (S-family token) on a controller faceplate
- **THEN** the physical view renders a switch cell showing its current state (on/off glyph), and the cell is hit-testable like other components.

#### Scenario: Knob and encoder state renders on the physical view

- **WHEN** a patch declares a knob or encoder on a controller faceplate
- **THEN** the physical view renders its cell with the current value percentage.

#### Scenario: Button state renders on the physical view

- **WHEN** a patch declares a button on a controller faceplate
- **THEN** the physical view renders its cell with the current on/off state.

## ADDED Requirements

### Requirement: Switch cell placement on faceplates

The system SHALL place switch cells on the physical view according to the controller's geometry data, mirroring how knobs/encoders/buttons are placed; a switch token whose controller faceplate has no switch geometry SHALL NOT collapse onto a neighboring control's cell (e.g. a knob's) — it either occupies its own cell or is not rendered, never mis-rendered as another control kind.

#### Scenario: Switch does not collapse onto a knob cell

- **WHEN** a patch declares a switch token for a controller whose faceplate geometry lacks a matching switch cell
- **THEN** the switch is never drawn over the knob's cell; it renders on its own cell when geometry provides one, otherwise it is omitted without affecting the knob's rendering.