# Spec Delta

## MODIFIED Requirements

### Requirement: 1:1 main view

The system SHALL render components onto the physical grid cells as the main view: a uniform mm→screen mapping with one isotropic scale — the same points per millimetre on both axes, so panel proportions stay true and a round panel element renders round — with 100 % zoom meaning true size at the 96-dpi reference, i.e. one millimetre equals 3.7795 points, so a 25.4 mm module is 96 points wide. The supported zoom range SHALL reach from 0.25 to 4.0 of that baseline, and the view SHALL provide zoom and pan for racks exceeding the pane; hit-testing publishes component rects matching the rendered cells. The view SHALL open fitted to its pane, centring the rack. Module borders MUST abut without overlapping: the mm→screen mapping and cell-rect rounding MUST NOT cause adjacent module borders to share or cross at any supported zoom level.

#### Scenario: Components land on their grid cells

- **WHEN** the full view renders
- **THEN** every rendered component rect equals its grid-model cell under the same scale and offset.

#### Scenario: 100 % zoom is true size

- **WHEN** the zoom is 100 % and the scale factor is 1
- **THEN** a 25.4 mm module renders 96 points wide and a 9 mm button cell renders 34 points square

#### Scenario: Square panel cell maps square

- **WHEN** a controller's geometry declares a square element cell (for example a 9 × 9 mm P2B8 or B32 button)
- **THEN** its rendered rect is square at every zoom preset, and a round face drawn on it is a circle

#### Scenario: Opens fitted to width

- **WHEN** the view opens in a pane
- **THEN** the zoom is chosen so the whole rack fits the pane's width and height, and the rack is centred in the pane

#### Scenario: A large rack can be seen whole

- **WHEN** a 400 mm rack is fitted into a 640-point-wide pane
- **THEN** the fit zoom is about 42 %, which is inside the supported range

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

### Requirement: Physical cell rendering contract (compact-only)

The system SHALL render physical-view element cells under a single compact-cell contract: a component cell always draws its state glyph, the label shares the first row at a fixed screen-space font size independent of the physical zoom — ellipsized to the cell width — when the cell is wide enough for at least one character of that font, and SHALL be omitted when the cell is narrower than one character of the label font, in which case the element's label is reachable through the label presentation and the hover reveal rather than being drawn too small; the state text takes the second row when the cell is tall enough. A composed button face (the `module-ui` capability's "Button face and LED composition") SHALL instead draw its state shape with its label beneath it when the cell has room for a label line, and SHALL omit the state-text row. The boxed-LED presentation path is removed. An LED that belongs to an element SHALL render inside that element's cell as defined by the `module-ui` capability and SHALL NOT render as a co-located or standalone cell; the element rect equals the compact cell.

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
- **THEN** the cell draws its state glyph without a label, no text smaller than the label font is drawn in its place, and the full label remains available on hover and through the label presentation

#### Scenario: No text below the legibility floor

- **WHEN** the view renders at any supported zoom with any label presentation
- **THEN** every text run it draws is at least 8 points
