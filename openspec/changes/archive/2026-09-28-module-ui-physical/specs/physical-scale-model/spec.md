# Spec Delta

## MODIFIED Requirements

### Requirement: Physical cell rendering contract (compact-only)

The system SHALL render physical-view element cells under a single compact-cell contract: a component cell always draws its state glyph, the label shares the first row when the cell is wide enough (ellipsized), and the state text takes the second row when the cell is tall enough. The boxed-LED presentation path is removed. An LED that belongs to an element SHALL render inside that element's cell as defined by the `module-ui` capability and SHALL NOT render as a co-located or standalone cell; the element rect equals the compact cell.

#### Scenario: LED element renders the compact cell

- **WHEN** a patch declares an LED-associated element
- **THEN** the physical view renders one compact cell for the element with the LED state inside it, and no separate LED cell

#### Scenario: Boxed-LED branch is unreachable

- **WHEN** the physical view renders any element cell at any zoom level
- **THEN** no element cell is drawn through the boxed-LED path

### Requirement: 1:1 main view

The system SHALL render components onto the physical grid cells as the main view: a uniform mm→screen mapping with aspect compensation, zoom levels, and pan/scroll for racks exceeding the pane; hit-testing publishes component rects matching the rendered cells. The view SHALL open fitted to the pane width. Module borders MUST abut without overlapping: the mm→screen mapping and cell-rect rounding MUST NOT cause adjacent module borders to share or cross at any supported zoom level.

#### Scenario: Components land on their grid cells

- **WHEN** the full view renders
- **THEN** every rendered component rect equals its grid-model cell under the same scale and offset.

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
