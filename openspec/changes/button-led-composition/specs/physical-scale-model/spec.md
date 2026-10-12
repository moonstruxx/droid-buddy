## MODIFIED Requirements

### Requirement: 1:1 main view

The system SHALL render components onto the physical grid cells as the main view: a uniform mm→screen mapping with one isotropic scale — the same points per millimetre on both axes, so panel proportions stay true and a round panel element renders round — zoom levels, and pan/scroll for racks exceeding the pane; hit-testing publishes component rects matching the rendered cells. The view SHALL open fitted to the pane width. Module borders MUST abut without overlapping: the mm→screen mapping and cell-rect rounding MUST NOT cause adjacent module borders to share or cross at any supported zoom level.

#### Scenario: Components land on their grid cells

- **WHEN** the full view renders
- **THEN** every rendered component rect equals its grid-model cell under the same scale and offset.

#### Scenario: Square panel cell maps square

- **WHEN** a controller's geometry declares a square element cell (for example a 9 × 9 mm P2B8 or B32 button)
- **THEN** its rendered rect is square at every zoom preset, and a round face drawn on it is a circle

#### Scenario: Opens fitted to width

- **WHEN** the view opens in a pane
- **THEN** the zoom is chosen so the full rack width fits the pane

#### Scenario: Overflow pans

- **WHEN** the rack is wider or taller than the pane after zooming in
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

The system SHALL render each element's state on its physical-view cell: buttons whose controller lights the LED in the button face show the composed state shape defined by the `module-ui` capability, other buttons and switches show their toggle glyph, knobs/encoders show their percentage, faders show a vertical track proportional to value with an amber LED bar, and CV I/O shows its direction — mirroring the panel view's state rendering, so the physical view is a faithful 1:1 representation of the patch's live state.

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
- **THEN** the physical view renders its cell as the composed state shape on a controller whose LED lights the button face, and as the toggle glyph on any other controller, and the cell is hit-testable

### Requirement: Physical cell rendering contract (compact-only)

The system SHALL render physical-view element cells under a single compact-cell contract: a component cell always draws its state glyph, the label shares the first row at a fixed screen-space font size independent of the physical zoom — ellipsized to the cell width — when the cell is wide enough for at least one character of that font, and SHALL be omitted when the cell is narrower than one character of the label font; the state text takes the second row when the cell is tall enough. A composed button face (the `module-ui` capability's "Button face and LED composition") SHALL instead draw its state shape with its label beneath it when the cell has room for a label line, and SHALL omit the state-text row. The boxed-LED presentation path is removed. An LED that belongs to an element SHALL render inside that element's cell as defined by the `module-ui` capability and SHALL NOT render as a co-located or standalone cell; the element rect equals the compact cell.

#### Scenario: Label size independent of physical zoom

- **WHEN** the user cycles the physical zoom presets from 75 % to 200 %
- **THEN** every rendered cell label uses the same font size in points while the cells scale with the zoom

#### Scenario: LED element renders the compact cell

- **WHEN** a patch declares an LED-associated element
- **THEN** the physical view renders one compact cell for the element with the LED state inside it, and no separate LED cell

#### Scenario: Composed face draws shape and label

- **WHEN** a composed button face renders in a cell tall enough for a label line
- **THEN** the cell draws the state shape and the label under it, and draws no state-text row

#### Scenario: Boxed-LED branch is unreachable

- **WHEN** the physical view renders any element cell at any zoom level
- **THEN** no element cell is drawn through the boxed-LED path

#### Scenario: Narrow cell hides the label

- **WHEN** a cell is narrower than one character of the label font at any zoom preset
- **THEN** the cell draws its state glyph without a label, and the full label remains available on hover and in the status bar
