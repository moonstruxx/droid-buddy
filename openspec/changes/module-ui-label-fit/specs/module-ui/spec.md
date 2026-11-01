# Spec Delta

## MODIFIED Requirements

### Requirement: Shift-aware element labels

Every element the patch references SHALL show its label, resolved for the active shift layer with the existing label fallback (stored label for the layer, then layer 1, then the patch preamble, then the derived name). Changing the shift layer SHALL update every label immediately. A label SHALL be fitted to its cell: the view SHALL choose, per cell, the largest step of a descending font ladder (10, 9, 8, 7, and 6 pt, monospace) at which the whole label fits the cell's available text box, wrapping at word boundaries, breaking a word only when a single word exceeds the line width, and ellipsizing on the last line only when the whole label cannot be drawn at the ladder's floor size. The in-cell state text SHALL yield to the label: it SHALL be drawn only when a ladder step leaves room for it alongside the label. A tall-narrow fader cell SHALL stack the label one glyph per line in a column beside a track narrowed to 40 % of the cell width. No text SHALL be rendered below 6 pt anywhere on the surface. An element whose cell cannot hold its label even at the floor size — a 3 mm LED cell, or any cell at extreme zoom — SHALL keep its label reachable through the label presentation and the hover reveal, so no addressed element's written label is unavailable at any zoom.

#### Scenario: Shift changes labels

- **WHEN** B1.1 has different labels for Group 1 and Group 2 and the user presses `2`
- **THEN** B1.1 shows its Group 2 label

#### Scenario: A long label wraps instead of disappearing

- **WHEN** a written label of 13 characters is set on a 9 mm button cell at 100 % zoom
- **THEN** the label renders in several lines at the largest ladder step that fits, whole and not ellipsized

#### Scenario: A small cell takes a smaller step

- **WHEN** a written label is set on a 7 mm jack at 100 % zoom
- **THEN** it renders at a ladder step below the 10 pt cell font, and no smaller than 6 pt

#### Scenario: A fader label stacks

- **WHEN** a P8S8 fader cell carries a written label
- **THEN** the label renders one glyph per line in the column beside the narrowed track

#### Scenario: The label beats the state row

- **WHEN** a cell can hold its label or its state row but not both
- **THEN** the label is drawn and the state row is omitted

#### Scenario: No text below the floor

- **WHEN** the view renders at any supported zoom with any label presentation
- **THEN** every text run it draws is at least 6 points

#### Scenario: Long label on hover

- **WHEN** an element's label is ellipsized, wrapped, or hidden and the pointer hovers the element
- **THEN** the full label appears in the hover callout and in the status line of the module-UI pane

#### Scenario: Narrow cell hides the label in-cell but not from the user

- **WHEN** a cell is narrower than one 6 pt character of the label font at any zoom preset
- **THEN** the cell draws its state without text, and the element's full label is available through the hover reveal in every label-presentation mode

#### Scenario: A label no step can hold stays reachable

- **WHEN** a 3 mm LED cell carries a written label that cannot fit at 6 pt in the cell
- **THEN** the cell omits the label, and the label is still available through its annotation and the hover reveal

### Requirement: Fit-width default

The module UI SHALL fit the rack to its pane — scaling so the whole rack width AND height are visible, and centring the rack in the pane — when the view opens, when a patch loads, when the rack definition changes, and when the pane is resized. The fitted zoom SHALL be the greater of that rack fit and the label fit — the smallest zoom at which every written label's cell yields an in-cell layout — clamped to the supported zoom range, so the default view shows every written label and the rack MAY exceed the pane and pan. A cell whose label cannot fit even at the largest supported zoom SHALL NOT lift the fit. After the user zooms or pans the view, these triggers SHALL leave the view alone until the user requests a fit again. A fit request SHALL always be available; `f` SHALL request the label fit, `Shift+F` SHALL request the rack fit, and the reported fit SHALL name which one applied.

#### Scenario: Rack fits the pane on open

- **WHEN** the module UI opens in a pane and every written label fits at the rack fit zoom
- **THEN** the whole rack is visible without panning, centred, and the fit zoom is reported as the label fit

#### Scenario: Labels lift the fit

- **WHEN** a patch's written labels do not fit at the zoom that would show the whole rack
- **THEN** the view opens at the label-fit zoom, the rack overflows the pane, and panning reveals the rest

#### Scenario: An impossible label does not lift the fit

- **WHEN** the only labels that do not fit at the rack fit zoom belong to cells that cannot hold them at any supported zoom
- **THEN** the view opens at the rack fit zoom and those labels are annotated instead

#### Scenario: Manual navigation is preserved

- **WHEN** the user zooms or pans the module UI and then the pane is resized
- **THEN** the zoom and pan the user set are unchanged

#### Scenario: Fit applies on request

- **WHEN** the user requests a fit after zooming and panning
- **THEN** the rack is scaled to the requested fit and centred again, and the report names labels or rack

#### Scenario: The rack fit is always available

- **WHEN** the user presses `Shift+F` while the view is at the label fit
- **THEN** the view returns to the whole-rack fit, centring the rack, and reports the rack fit

### Requirement: Label presentation modes

The module UI SHALL provide a label presentation with three modes — `off`, `auto`, and `on` — cycled by a user action and persisted as a `[physical]` setting whose absent value is `auto`.

- In `off` the view SHALL render labels inside their cells only, as the element-label contract describes.
- In `auto` the view SHALL annotate exactly those addressed elements whose in-cell label has no in-cell layout at the current zoom — the cell could not fit it even at the ladder floor — and whose label is explicitly set (a stored label or a patch preamble label): never an element whose label is drawn in its cell, never an element of the patch that the controller does not address, and never an element whose label is only the derived token name. Annotations SHALL be drawn outside the rack with a leader line to the annotated cell and SHALL use the same presentation as the performance view's callouts. When the deepest annotation band would exceed 30 % of the pane on either axis, the view SHALL drop annotations in a fixed priority order until the bands fit: LEDs and jacks first, then buttons, then switches, then pots, then faders, and never the focused element or the hardware of the selected circuit.
- In `on` the view SHALL annotate every addressed element that carries a written label, with no dropping.

Annotations SHALL be decorative: they SHALL publish no hit rect, SHALL NOT alter the drawn cell rects or the published component hit rects, and SHALL NOT change what the skeleton presentation draws. The view SHALL report how many labels are displayed and, when any were dropped or hidden, how many remain available through the hover reveal.

#### Scenario: Auto annotates only what the cell cannot show

- **WHEN** the label presentation is `auto` at true 1:1 and the rack holds a 12 mm pot whose written label fits at a ladder step and a 3 mm LED whose written label fits at none
- **THEN** the pot renders its label inside its cell with no annotation, and the LED is annotated outside the rack with a leader line to its cell

#### Scenario: Derived labels stay reveal-only

- **WHEN** the label presentation is `auto` and an element carries no stored or preamble label, only its derived token name
- **THEN** the element gets no annotation, and the full name still appears through the hover reveal

#### Scenario: Unaddressed elements are never annotated

- **WHEN** the controller declares elements the patch does not address
- **THEN** those elements render dimmed and unlabelled, and no annotation names them

#### Scenario: Budget drops in priority order

- **WHEN** a dense patch's annotation bands would exceed 30 % of the pane height
- **THEN** annotations are dropped starting with LEDs and jacks, the focused element and the selected circuit's hardware keep theirs, and the status reports the displayed and hidden counts

#### Scenario: Off keeps the cells alone

- **WHEN** the label presentation is `off`
- **THEN** no annotation is drawn and only fitted in-cell labels appear

#### Scenario: On annotates everything labelled

- **WHEN** the label presentation is `on`
- **THEN** every addressed element with a written label is annotated, whether or not its cell could hold it

#### Scenario: Annotations are decoration

- **WHEN** annotations are drawn over a patch
- **THEN** the published component hit rects and the skeleton rects are byte-identical to the same view with the presentation `off`

#### Scenario: Presentation survives a patch load

- **WHEN** the user selects a presentation and loads another patch
- **THEN** the selected presentation still applies

### Requirement: Button face and LED composition

A button-kind element whose controller lights its LED in the button face SHALL render as a state shape instead of a state glyph: an outline whose colour carries the button's on/off state, with the folded LED drawn as an inner fill whose extent and colour follow the LED level. A round face (P2B8, B32) SHALL render a circle and a rectangular face (the M4 touch plate) SHALL render a rectangle. The element's label SHALL render in the band beneath the shape at the largest font-ladder step that fits that band, in up to two lines when the band's height allows, and SHALL be omitted from the cell — remaining an annotation candidate — when no step fits; the shape SHALL NOT be shrunk or displaced to make room for the label. The element's state text SHALL NOT render in the cell, because the shape carries the state. An element with no bound LED SHALL render the shape alone. A button on any other controller SHALL keep the glyph rendering.

#### Scenario: P2B8 button renders a state circle

- **WHEN** a patch declares a P2B8 button
- **THEN** its cell renders a circle whose outline colour follows the button's on/off state, with no state glyph and no state-text row

#### Scenario: B32 button LED fills the circle

- **WHEN** a B32 button's LED is at full level
- **THEN** the circle is filled to its full extent in the LED's brightness, and at the floor level it shows only a small dim core

#### Scenario: M4 touch plate fills as a rectangle

- **WHEN** a patch declares an M4 touch plate
- **THEN** its cell renders a rectangle outline with the LED as an inner rectangle fill, not a circle

#### Scenario: Button without an LED

- **WHEN** a button on a composed controller has no bound LED
- **THEN** the cell renders the state shape alone with no fill

#### Scenario: Label under a roomy shape

- **WHEN** a composed cell's label band beneath the shape fits a ladder step
- **THEN** the label renders under the shape at that step, wrapped to at most two lines, and never past the cell width

#### Scenario: The face keeps its size for the label

- **WHEN** a composed cell's label cannot fit in the band beneath the shape at 6 pt
- **THEN** the cell renders the shape alone and the label is available through its annotation and the hover reveal

#### Scenario: Out-of-scope controller keeps its glyph

- **WHEN** a patch declares a button on a controller whose LED does not light the button face (P4B2, master, G8, X7)
- **THEN** the cell renders the state glyph as before
