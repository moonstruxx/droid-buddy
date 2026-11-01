# Spec Delta

## MODIFIED Requirements

### Requirement: 1:1 main view

The system SHALL render components onto the physical grid cells as the main view: a uniform mm→screen mapping with one isotropic scale — the same points per millimetre on both axes, so panel proportions stay true and a round panel element renders round — with 100 % zoom meaning true size at the 96-dpi reference, i.e. one millimetre equals 3.7795 points, so a 25.4 mm module is 96 points wide. The supported zoom range SHALL reach from 0.25 to 4.0 of that baseline, and the view SHALL provide zoom and pan for racks exceeding the pane; hit-testing publishes component rects matching the rendered cells. The view SHALL open at the fitted zoom — the greater of the zoom that fits the whole rack in the pane and the zoom at which every written element label fits its cell, clamped to the supported range — centring the rack, so a rack that cannot be shown whole at a readable zoom opens overflowing and pans. Module borders MUST abut without overlapping: the mm→screen mapping and cell-rect rounding MUST NOT cause adjacent module borders to share or cross at any supported zoom level.

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
- **THEN** the zoom is the greater of the whole-rack fit and the written-label fit, clamped to the supported range, and the rack is centred in the pane

#### Scenario: A large rack can be seen whole

- **WHEN** a 400 mm rack is fitted to the pane with the rack fit (`Shift+F`) in a 640-point-wide pane
- **THEN** the fit zoom is about 42 %, which is inside the supported range, and panning is not needed to see the whole rack

#### Scenario: A large rack opens readable and pans

- **WHEN** a rack's written labels need a larger zoom than the whole-rack fit
- **THEN** the view opens at the label fit, the rack exceeds the pane, and panning reveals the rest

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

The system SHALL render physical-view element cells under a single compact-cell contract: a component cell draws its state glyph, and its label is fitted to the cell by a descending font ladder (10, 9, 8, 7, and 6 pt, monospace) — the largest step at which the whole label fits the cell's available text box, wrapped at word boundaries, with an ellipsis on the last line only when the label cannot be drawn whole at the floor size. The state text occupies its own row only when a ladder step leaves room for it alongside the label; otherwise the label takes the cell and the state row is omitted. A label that no step fits SHALL be omitted from the cell, and the element's label remains reachable through the label presentation and the hover reveal. A tall-narrow fader cell SHALL narrow its track to 40 % of the cell width and stack the label one glyph per line in the remaining column, and SHALL keep its vertical value strip and amber bar. A composed button face (the `module-ui` capability's "Button face and LED composition") SHALL instead draw its state shape with its label in the band beneath it, and SHALL omit the state-text row. No text SHALL be rendered below 6 pt. The boxed-LED presentation path is removed. An LED that belongs to an element SHALL render inside that element's cell as defined by the `module-ui` capability and SHALL NOT render as a co-located or standalone cell; the element rect equals the compact cell.

#### Scenario: Label size independent of physical zoom

- **WHEN** the user cycles the physical zoom presets from 75 % to 200 %
- **THEN** each cell's label step is chosen from that cell's own rendered size at that zoom — a cell that grows may take a larger step — and no run drops below 6 pt

#### Scenario: Wrap before shrink

- **WHEN** a label does not fit on one line at the current step but fits when wrapped
- **THEN** the cell wraps the label at that step instead of stepping the font down

#### Scenario: LED element renders the compact cell

- **WHEN** a patch declares an LED-associated element
- **THEN** the physical view renders one compact cell for the element with the LED state inside it, and no separate LED cell

#### Scenario: Composed face draws shape and label

- **WHEN** a composed button face renders in a cell whose band beneath the shape fits a ladder step
- **THEN** the cell draws the state shape and the label beneath it, and draws no state-text row

#### Scenario: Fader stacks its label

- **WHEN** a fader cell is narrower than its label needs horizontally
- **THEN** the track narrows and the label stacks one glyph per line beside it, no smaller than 6 pt

#### Scenario: Boxed-LED branch is unreachable

- **WHEN** the physical view renders any element cell at any zoom level
- **THEN** no element cell is drawn through the boxed-LED path

#### Scenario: Narrow cell hides the label

- **WHEN** a cell is narrower than one 6 pt character of the label font at any zoom preset
- **THEN** the cell draws its state without a label, no text smaller than the floor is drawn in its place, and the full label remains available on hover and through the label presentation

#### Scenario: No text below the legibility floor

- **WHEN** the view renders at any supported zoom with any label presentation
- **THEN** every text run it draws is at least 6 points
