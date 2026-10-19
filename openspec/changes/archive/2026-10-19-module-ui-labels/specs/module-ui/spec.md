# Spec Delta

## MODIFIED Requirements

### Requirement: Shift-aware element labels

Every element the patch references SHALL show its label, resolved for the active shift layer with the existing label fallback (stored label for the layer, then layer 1, then the patch preamble, then the derived name). Changing the shift layer SHALL update every label immediately. A label SHALL render inside its cell at the fixed cell-label font when the cell is wide enough for at least one character of that font and SHALL be ellipsized to the cell width; when the cell cannot hold the label, the element's label SHALL remain reachable through the label presentation (its annotation callout) and through the hover reveal, so no addressed element's label is unavailable at any zoom. Text SHALL NOT be rendered below 8 pt anywhere on the surface.

#### Scenario: Shift changes labels

- **WHEN** B1.1 has different labels for Group 1 and Group 2 and the user presses `2`
- **THEN** B1.1 shows its Group 2 label

#### Scenario: Long label on hover

- **WHEN** an element's label is ellipsized or hidden and the pointer hovers the element
- **THEN** the full label appears in the hover callout and in the status line of the module-UI pane

#### Scenario: Narrow cell hides the label in-cell but not from the user

- **WHEN** a cell is narrower than one character of the label font at any zoom preset
- **THEN** the cell draws its state without text, and the element's full label is available through the hover reveal in every label-presentation mode

### Requirement: Fit-width default

The module UI SHALL fit the rack to its pane — scaling so the whole rack width AND height are visible, and centring the rack in the pane — when the view opens, when a patch loads, when the rack definition changes, and when the pane is resized. After the user zooms or pans the view, these triggers SHALL leave the view alone until the user requests a fit again. A fit request SHALL always be available and SHALL apply the same computation.

#### Scenario: Rack fits the pane on open

- **WHEN** the module UI opens in a pane
- **THEN** the full rack is visible without panning, centred, and the fit zoom is reported

#### Scenario: Manual navigation is preserved

- **WHEN** the user zooms or pans the module UI and then the pane is resized
- **THEN** the zoom and pan the user set are unchanged

#### Scenario: Fit applies on request

- **WHEN** the user requests a fit after zooming and panning
- **THEN** the rack is visible in full and centred again

### Requirement: Module UI interactions

The module UI SHALL support the element interactions the Panels view provided: keyboard navigation between elements, Enter/Space to toggle or select, `Ctrl+Enter` to toggle the focused element's folded LED without changing the element's own state, mouse wheel to adjust knobs and faders, holding and latching a modifier (`m`, `Ctrl+Shift+Click`) with the modifier wash, the label editor (`e`), shift groups `1`–`4`, and highlighting the hardware of the selected circuit. The pointer gestures SHALL be disambiguated by the hit cell and the modifier: a wheel over an adjustable element adjusts its value, a wheel over the pane but not over an adjustable element zooms about the pointer, `Ctrl`+wheel zooms about the pointer wherever the pointer is, and a middle-button drag pans the view. None of these gestures SHALL change which element is selected or focus.

#### Scenario: Label editor from the module UI

- **WHEN** an element is focused in the module UI and the user presses `e`
- **THEN** the label editor opens for that element's token

#### Scenario: Wheel adjusts a fader

- **WHEN** the pointer is over a fader element and the user scrolls
- **THEN** the fader value changes

#### Scenario: Wheel off an element zooms

- **WHEN** the pointer is over the pane but not over an adjustable element and the user scrolls
- **THEN** the view zooms about the pointer

#### Scenario: Modified wheel zooms over an element

- **WHEN** the pointer is over an adjustable element and the user scrolls with `Ctrl` held
- **THEN** the view zooms about the pointer and the element's value is unchanged

#### Scenario: Middle-drag pans

- **WHEN** the user drags with the middle button over the module UI
- **THEN** the view pans and no element is selected or changed

#### Scenario: LED toggle leaves the element state alone

- **WHEN** an element with a folded LED is focused and the user presses `Ctrl+Enter`
- **THEN** the folded LED's state toggles and the element's own state is unchanged

## ADDED Requirements

### Requirement: Label presentation modes

The module UI SHALL provide a label presentation with three modes — `off`, `auto`, and `on` — cycled by a user action and persisted as a `[physical]` setting whose absent value is `auto`.

- In `off` the view SHALL render labels inside their cells only, as the compact-cell contract describes.
- In `auto` the view SHALL annotate exactly those addressed elements whose in-cell label is clipped or hidden at the current zoom and whose label is explicitly set (a stored label or a patch preamble label) — never an element whose label already fits inside its cell, never an element of the patch that the controller does not address, and never an element whose label is only the derived token name. Annotations SHALL be drawn outside the rack with a leader line to the annotated cell and SHALL use the same presentation as the performance view's callouts. When the deepest annotation band would exceed 30 % of the pane on either axis, the view SHALL drop annotations in a fixed priority order until the bands fit: LEDs and jacks first, then buttons, then switches, then pots, then faders, and never the focused element or the hardware of the selected circuit.
- In `on` the view SHALL annotate every addressed element that carries a label, with no dropping.

Annotations SHALL be decorative: they SHALL publish no hit rect, SHALL NOT alter the drawn cell rects or the published component hit rects, and SHALL NOT change what the skeleton presentation draws. The view SHALL report how many labels are displayed and, when any were dropped or hidden, how many remain available through the hover reveal.

#### Scenario: Auto annotates only what the cell cannot show

- **WHEN** the label presentation is `auto` at true 1:1 and the rack holds a 12 mm pot whose label is short and a 9 mm button whose label is long
- **THEN** the pot renders its label inside its cell with no annotation, and the button is annotated outside the rack with a leader line to its cell

#### Scenario: Derived labels stay reveal-only

- **WHEN** the label presentation is `auto` and an element carries no stored or preamble label, only its derived token name
- **THEN** the element gets no annotation even though its cell cannot show the derived name, and the full name still appears through the hover reveal

#### Scenario: Unaddressed elements are never annotated

- **WHEN** the controller declares elements the patch does not address
- **THEN** those elements render dimmed and unlabelled, and no annotation names them

#### Scenario: Budget drops in priority order

- **WHEN** a dense patch's annotation bands would exceed 30 % of the pane height
- **THEN** annotations are dropped starting with LEDs and jacks, the focused element and the selected circuit's hardware keep theirs, and the status reports the displayed and hidden counts

#### Scenario: Off keeps the cells alone

- **WHEN** the label presentation is `off`
- **THEN** no annotation is drawn and only in-cell labels appear

#### Scenario: On annotates everything labelled

- **WHEN** the label presentation is `on`
- **THEN** every addressed element with a label is annotated, whether or not its cell could hold it

#### Scenario: Annotations are decoration

- **WHEN** annotations are drawn over a patch
- **THEN** the published component hit rects and the skeleton rects are byte-identical to the same view with the presentation `off`

#### Scenario: Presentation survives a patch load

- **WHEN** the user selects a presentation and loads another patch
- **THEN** the selected presentation still applies

### Requirement: Element label reveal

The module UI SHALL reveal the full label of the element under the pointer, together with its state, as a callout anchored to that element's cell, in every label-presentation mode and in both the full and skeleton presentations. The callout SHALL be positioned inside the pane (flipping side near a pane edge), SHALL publish no hit rect, and SHALL NOT hide the element it describes. With no element under the pointer no callout SHALL be drawn. The same information SHALL be reported in the module-UI pane's status line for the element the pointer is over, falling back to the focused element when the pointer is elsewhere.

#### Scenario: Hover reveals a clipped label

- **WHEN** the label presentation is `off` and the pointer hovers an element whose label does not fit its cell
- **THEN** a callout shows the element's full label and state beside its cell

#### Scenario: Callout stays inside the pane

- **WHEN** the pointer hovers an element at the pane's right edge
- **THEN** the callout is drawn on the side of the cell that keeps it inside the pane

#### Scenario: Leaving the pane clears the reveal

- **WHEN** the pointer leaves the module-UI pane
- **THEN** no callout is drawn
