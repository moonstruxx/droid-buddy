# physical-scale-model Specification

## Purpose
Render a patch's hardware as a 1:1 physical scale model of the DROID master + controller chain: modules at real sizes (mm derived from HP), elements at real faceplate positions, in the order the patch declares them. A geometry-only skeleton render serves as the validation reference; the full render must coincide with it.

## Requirements

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

The system SHALL render each element's state on its physical-view cell: buttons/switches show their toggle glyph, knobs/encoders show their percentage, faders show a vertical track proportional to value with an amber LED bar, and CV I/O shows its direction — mirroring the panel view's state rendering, so the physical view is a faithful 1:1 representation of the patch's live state.

#### Scenario: Switch state renders on the physical view

- **WHEN** a patch declares a switch (S-family token) on a controller faceplate
- **THEN** the physical view renders a switch cell showing its current state (on/off glyph), and the cell is hit-testable like other components.

#### Scenario: Knob and encoder state renders on the physical view

- **WHEN** a patch declares a knob or encoder on a controller faceplate
- **THEN** the physical view renders its cell with the current value percentage.

#### Scenario: Fader state renders as a vertical track

- **WHEN** a patch declares a fader (F-family, P register) on an M4 or P8S8 controller
- **THEN** the physical view renders its cell as a vertical track proportional to value, with an amber LED bar mirroring position, and the cell is hit-testable.

#### Scenario: Button state renders on the physical view

- **WHEN** a patch declares a button on a controller faceplate
- **THEN** the physical view renders its cell with the current on/off state.

### Requirement: Physical cell rendering contract (compact-only)

The system SHALL render physical-view element cells under a single compact-cell contract: a component cell always draws its state glyph, the label shares the first row when the cell is wide enough (ellipsized), and the state text takes the second row when the cell is tall enough. The boxed-LED presentation path gated on `width>=5 && height>=3` is removed; LED-associated elements render the compact cell with the LED as a co-located cell, and the element rect equals the compact cell.

#### Scenario: LED element renders the compact cell

- **WHEN** a patch declares an LED-associated element
- **THEN** the physical view renders the compact cell (glyph + truncated state) with the LED as a co-located cell, and never the boxed-LED branch (which requires a cell of width ≥ 5 and height ≥ 3 that no geometry cell reaches at any zoom preset).

#### Scenario: Boxed-LED branch is unreachable

- **WHEN** the physical view renders any element cell at zoom 75%, 100%, 150%, or 200%
- **THEN** no element cell is drawn through the boxed-LED path; every LED element uses the compact contract.

### Requirement: Switch cell placement on faceplates

The system SHALL place switch cells on the physical view according to the controller's geometry data, mirroring how knobs/encoders/buttons are placed; a switch token whose controller faceplate has no switch geometry SHALL NOT collapse onto a neighboring control's cell (e.g. a knob's) — it either occupies its own cell or is not rendered, never mis-rendered as another control kind.

#### Scenario: Switch does not collapse onto a knob cell

- **WHEN** a patch declares a switch token for a controller whose faceplate geometry lacks a matching switch cell
- **THEN** the switch is never drawn over the knob's cell; it renders on its own cell when geometry provides one, otherwise it is omitted without affecting the knob's rendering.

### Requirement: Rack model

The system SHALL model the case/rack as an ordered set of rows — each with a height (HE: 1 or 3), a width (HP), and an optional label — plus optional top-mount and side-mount sections (in TE); modules are assigned to rows by auto-packing in chain order, overridable per module.

#### Scenario: Auto-pack fills rows in chain order

- **WHEN** a rack has rows and a chain of controllers
- **THEN** controllers pack left-to-right into row 0 until the next module would exceed the row's HP, which then starts row 1, and so on.

#### Scenario: Per-module override

- **WHEN** the config assigns a module to a specific row
- **THEN** the module is placed in that row regardless of auto-pack; an out-of-range override falls back to auto-pack.

#### Scenario: User-defined case

- **WHEN** the user configures `[physical.rack]` with rows and TE mounts
- **THEN** the physical view renders the case structure (rows, top/side mount regions) with the modules inside.

#### Scenario: Default case

- **WHEN** no rack config is given
- **THEN** a single row wide enough for the whole chain is used.

### Requirement: Coincidence verification

The system SHALL verify the full render against the skeleton: tests assert every full-render element rect coincides with its skeleton cell, and the gallery matrix renders skeleton | full side by side.

#### Scenario: Full render coincides with skeleton

- **WHEN** both renders are produced for the same patch at the same viewport
- **THEN** every full-render element rect equals the corresponding skeleton cell.

#### Scenario: Gallery proves fidelity

- **WHEN** the gallery matrix is generated
- **THEN** each physical-layout scenario includes a skeleton row and a full row for side-by-side proof.

### Requirement: DB8E OLED display placeholder

The system SHALL render a bordered OLED display placeholder in the upper band of every DB8E faceplate on the physical 1:1 view, above the B-grid (above `y0_mm 38` inside the 6 HP module rect), with centered state text derived from the patch. The placeholder is part of the rack structure path so both skeleton and full presentations render it identically.

#### Scenario: Placeholder is visible in the DB8E upper band

- **WHEN** a patch contains a DB8E controller and the physical view renders
- **THEN** a single bordered rectangle appears in the upper band of that faceplate (above the B-grid), regardless of zoom or skeleton/full mode.

#### Scenario: Placeholder text is centered and derived from the patch

- **WHEN** the placeholder renders
- **THEN** its centered text is the derived DB8E display state: `"not used"` when the patch declares no DB8E, `"configuration error"` when the declared controller chain mismatches the wired chain (stub: otherwise `"connected"`), truncated/ellipsized to the rect width.

#### Scenario: Placeholder is rendered through the shared rack-structure path

- **WHEN** either `render_physical_skeleton` or `render_physical_full` draws the rack
- **THEN** the DB8E placeholder is drawn inside `render_rack_structure`'s per-module loop, so skeleton and full coincide (same rect, same text) per the D5 coincidence contract.

#### Scenario: Placeholder is not hit-testable

- **WHEN** the physical view publishes `component_rects` for handler hit-testing
- **THEN** the display placeholder does not publish a hit rect (it is decorative, not a component cell).
