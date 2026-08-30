# Controller Panels Specification

## Purpose

Render hardware components grouped by physical controller type (P2B8, Faderbank, Notebuttons, Encoder, etc.) in labeled panels that mirror the physical hardware layout, providing clear visual organization of patch components.

## Requirements

### Requirement: Group components by controller type
The system SHALL group hardware components by their physical controller type (P2B8, Faderbank, Notebuttons, Encoder, Pot, Unusedfaders, etc.) based on the hardware token prefix and position. Controller panels now contain modules rather than raw components directly.

#### Scenario: P2B8 components grouped
- **WHEN** a patch contains tokens B1.1-B1.8, L1.1-L1.8, P1.1-P1.2
- **THEN** all 18 components are rendered inside modules within a single panel labeled "P2B8"

#### Scenario: Faderbank components grouped
- **WHEN** a patch contains fader-related tokens
- **THEN** those components are rendered inside modules within a panel labeled "Faderbank"

### Requirement: Render controller panel with border and title
The system SHALL render each controller group as a bordered panel with a title showing the controller type name. Panels now contain module containers which in turn contain components.

#### Scenario: Panel with title
- **WHEN** a controller panel is rendered
- **THEN** it displays a border with the controller type name as the title (e.g., " P2B8 ", " Faderbank ")

#### Scenario: Panel contains modules
- **WHEN** a controller panel has components from multiple circuits
- **THEN** those components are first grouped into module containers, then the modules are arranged within the panel

### Requirement: Position components in physical layout order

The system SHALL position components by the physical grid model — real millimeter cells in chain order (modules sized from HP, elements at their faceplate positions) — rather than by wrapped panel rows. Repeated instances of the same controller render as separate side-by-side faceplates.

#### Scenario: P2B8 button order

- **WHEN** P2B8 buttons are rendered
- **THEN** B1.1 appears first (left), B1.8 appears last (right), in physical order within their module

#### Scenario: P2B8 components at physical cells

- **WHEN** a patch contains tokens B1.1-B1.8, L1.1-L1.8, P1.1-P1.2
- **THEN** the components are rendered at the P2B8 module's real faceplate cells (width in HP, element pitch from the geometry data), not in a uniform wrapped grid.

#### Scenario: Two instances render as two faceplates

- **WHEN** a patch declares two `[p2b8]` sections
- **THEN** two P2B8 faceplates render side by side at their real widths, in declaration order.

### Requirement: Display component labels and state
The system SHALL display each component's label (e.g., "TRIG A", "CUTOFF", "CV IN 1") and its current state (ON/OFF for buttons, percentage for knobs, value for CV I/O) within its panel. Labels longer than the available cell width SHALL truncate with a trailing ellipsis (…) while preserving cell geometry and hit-testing.

#### Scenario: Button state display
- **WHEN** a button component is rendered
- **THEN** it shows its label and current state (ON/OFF) with a visual indicator (● for on, ○ for off)

#### Scenario: Knob value display
- **WHEN** a knob component is rendered
- **THEN** it shows its label and current value as a percentage (e.g., "50%")

#### Scenario: Over-long label ellipsized
- **WHEN** a component's label exceeds the available cell width
- **THEN** the label truncates with a trailing ellipsis (…) and the cell geometry and hit rect are unchanged

### Requirement: Handle overflow with scrolling or wrapping

The system SHALL handle racks wider or taller than the terminal main area by panning/scrolling (the physical view), keeping the 1:1 mapping intact.

#### Scenario: Panel overflow

- **WHEN** a controller panel has more components than fit in the visible area
- **THEN** the view pans/scrolls to reveal the rest, keeping module geometry intact.

#### Scenario: Panel overflow with modules

- **WHEN** a controller chain has more modules than fit in the visible width
- **THEN** the view pans horizontally, maintaining each module's internal component layout.

#### Scenario: Rack wider than terminal pans

- **WHEN** the rack's total width exceeds the terminal main area
- **THEN** the view pans horizontally to reveal the rest, without compressing module geometry.

### Requirement: Support terminal resize
The system SHALL reflow the layout when the terminal is resized, preserving component visibility and panel structure.

#### Scenario: Terminal resized larger
- **WHEN** the terminal window is enlarged
- **THEN** panels expand to use available space, potentially showing more components per row

#### Scenario: Terminal resized smaller
- **WHEN** the terminal window is reduced
- **THEN** panels reflow to fit the smaller area, wrapping components as needed

### Requirement: Module-aware layout calculation
The system SHALL calculate panel layout based on module dimensions rather than fixed component counts. Panel width accommodates the sum of module widths plus spacing. Rows within a panel SHALL maintain uniform vertical spacing regardless of whether the cells they contain render as boxed (height 3) or unboxed two-line cells.

#### Scenario: Variable-width module arrangement
- **WHEN** a panel contains modules of different widths (4HP, 8HP, 12HP)
- **THEN** they are arranged left-to-right with each taking space proportional to its width

#### Scenario: Panel wraps at terminal boundary
- **WHEN** the sum of module widths exceeds terminal width
- **THEN** subsequent modules wrap to the next row within the panel

#### Scenario: Uniform vertical row spacing
- **WHEN** a panel mixes boxed and unboxed component cells in adjacent rows
- **THEN** rows maintain consistent vertical rhythm without irregular gaps

### Requirement: Box LED-associated elements
An element with an associated LED (`led: Some(...)`) SHALL render as a single bordered cell: border color from the active theme's token for the element's kind (button, knob, cv-in, cv-out, led; in the default `classic` theme these are button=white, knob=magenta, cv-in=cyan, cv-out=green, led=red); inside the border the element symbol, label, state, and the LED glyph reflecting the LED component's state. The LED component SHALL NOT render as its own standalone cell.

An element's LED association is detected from two sources in its `.ini` section: a bare `led = L.N` entry (single element per section), or a numbered circuit LED param `ledN = L.M` (e.g. `led11 = L1.1`) that shares its numeric suffix with a same-suffix element entry (`button11 = B1.1`, `pot11 = P1.1`, `encoder11 = E1.1`, `switch11 = S1.1`, `fader11 = M1.1`, ...) in the same section — the DROID convention for circuits like `matrixmixer` that address elements and LEDs by a shared matrix-position suffix. The `ledN` value (`L.M`) is authoritative for the LED hardware token; its serial-position-dependent numbering is encoded by the patch author, not derived by the parser.

The boxed path SHALL support every control kind that can carry a resolvable LED association — button, knob/pot, encoder, switch, fader — rendering each kind's state inside the box (ON/OFF with a glyph for buttons and switches, percentage for knobs/encoders/faders). When the available cell width is narrower than the box content, the cell SHALL either shrink its content to fit inside a complete box or fall back to unboxed two-line rendering; it SHALL NEVER emit partial border fragments.

#### Scenario: P2B8 button with LED
- **WHEN** a P2B8 button section carries an `led = L1.N` association and the classic theme is active
- **THEN** B1.1 renders as a bordered box with white border showing the button's symbol/label/state and the LED's glyph/state inside

#### Scenario: Numbered circuit LED param pairs button and LED
- **WHEN** a circuit section carries `button11 = B1.1` and `led11 = L1.1` (shared suffix `11`) and any theme is active
- **THEN** B1.1 renders as a single bordered box with L1.1's glyph folded inside, and L1.1 does not render as a standalone cell

#### Scenario: Encoder with LED
- **WHEN** a circuit section carries `encoder11 = E1.1` and `led11 = L1.2` (shared suffix `11`) and any theme is active
- **THEN** E1.1 renders as a single bordered box showing its percentage and the LED glyph, and L1.2 does not render as a standalone cell

#### Scenario: Switch with LED
- **WHEN** a circuit section carries `switch11 = S1.1` and `led11 = L1.3` (shared suffix `11`) and any theme is active
- **THEN** S1.1 renders as a single bordered box showing its switch glyph and the LED glyph, and L1.3 does not render as a standalone cell

#### Scenario: Fader with LED
- **WHEN** a circuit section carries `fader11 = M1.1` and `led11 = L1.4` (shared suffix `11`) and any theme is active
- **THEN** M1.1 renders as a single bordered box showing its value and the LED glyph, and L1.4 does not render as a standalone cell

#### Scenario: Knob without LED
- **WHEN** a pot has no LED association and any theme is active
- **THEN** it renders as the two-line text cell (no border, no box)

#### Scenario: Kind colors follow the theme
- **WHEN** any theme is active and an element with an LED association renders
- **THEN** its box border uses that theme's token for the element's kind

#### Scenario: Narrow width does not garble the box
- **WHEN** the available cell width is smaller than the box content for an LED-associated element
- **THEN** the cell renders with complete borders — content shrinks to fit or the cell falls back to unboxed two-line rendering, never partial border fragments

### Requirement: Box geometry and hit-testing

The system SHALL publish `component_rects` matching the rendered physical cells under the current scale and pan offset, preserving the renderer-owns-geometry contract.

#### Scenario: Click on boxed cell

- **WHEN** the user clicks anywhere inside a bordered box
- **THEN** the element toggles/selects

#### Scenario: LED state changes

- **WHEN** the LED state of a boxed element changes
- **THEN** the glyph inside the box updates accordingly

#### Scenario: Hit rects match rendered cells

- **WHEN** a component renders at a physical cell
- **THEN** its published `component_rects` entry covers exactly the rendered cell's screen area.

### Requirement: Panels render dim while processing paused

While global processing is paused, the panel area SHALL render with dimmed styling (all panel content, including boxes and text), the status bar SHALL show `PROCESSING PAUSED`, and a status message SHALL read `Processing paused (p to resume)`. Geometry, hit-testing, and module grouping are unchanged while paused.

#### Scenario: Paused panels dim

- **WHEN** processing is paused
- **THEN** the panel main area renders dimmed while the header and status bars render normally.

#### Scenario: Resume un-dims

- **WHEN** processing is resumed
- **THEN** panels render exactly as before pausing.

### Requirement: Controller panel component labels use overlay fallback
The system SHALL render each HW component cell label as `Patch::display_label(token, active_shift)` with the overlay fallback chain, preserving cell geometry and hit-testing.

#### Scenario: Panel shows store label
- **WHEN** active shift is Group 2 and `hw."B3.17".2` is set
- **THEN** the panel cell for `B3.17` shows the store label for layer 2, and `component_rects` for hit-testing is unchanged
